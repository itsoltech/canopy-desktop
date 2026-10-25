#import "CanopyGrpc.h"
#import "CanopySsh.h"
#import <CommonCrypto/CommonDigest.h>
#include <openssl_grpc/evp.h>
#include <openssl_grpc/ec.h>
#include <openssl_grpc/pem.h>
#include <openssl_grpc/x509.h>
#include <grpc/grpc.h>
#include <grpc/credentials.h>
#include <grpc/byte_buffer_reader.h>
#include <grpc/support/time.h>
#include <grpc/slice.h>
#include <map>
#include <memory>
#include <mutex>
#include <atomic>
#include <string>

static NSData *certificateDER(NSString *pem) {
  NSRange start = [pem rangeOfString:@"-----BEGIN CERTIFICATE-----"];
  NSRange end = [pem rangeOfString:@"-----END CERTIFICATE-----"];
  if (start.location == NSNotFound || end.location == NSNotFound || end.location <= NSMaxRange(start)) return nil;
  return [[NSData alloc] initWithBase64EncodedString:[pem substringWithRange:NSMakeRange(NSMaxRange(start), end.location - NSMaxRange(start))] options:NSDataBase64DecodingIgnoreUnknownCharacters];
}
static std::string fingerprint(NSData *data) {
  unsigned char digest[CC_SHA256_DIGEST_LENGTH]; CC_SHA256(data.bytes, (CC_LONG)data.length, digest);
  char hex[65]; for (int i = 0; i < 32; ++i) snprintf(hex + i * 2, 3, "%02x", digest[i]);
  return hex;
}
static int verifyPeer(const char *, const char *pem, void *expected) {
  @autoreleasepool {
    NSData *der = certificateDER([NSString stringWithUTF8String:pem]);
    return der && fingerprint(der) == *static_cast<std::string *>(expected) ? 0 : 1;
  }
}
struct Connection {
  grpc_channel *channel = nullptr;
  CanopySsh *ssh = nil;
  std::atomic<size_t> buffered{0};
  ~Connection() { if (channel) grpc_channel_destroy(channel); [ssh close]; }
};
struct Call {
  std::shared_ptr<Connection> connection;
  std::string host;
  grpc_call *call;
  grpc_completion_queue *queue;
  grpc_metadata_array initial{}, trailing{};
  grpc_status_code status = GRPC_STATUS_UNKNOWN;
  grpc_slice details = grpc_empty_slice();
  grpc_byte_buffer *received = nullptr, *sending = nullptr;
  size_t buffered = 0;
  bool statusReceived = false, eof = false, drained = false;
  bool reading = false, awaitingAck = false, writing = false, ended = false, closed = false, bidi = false, sent = false;
  std::mutex mutex;
  ~Call() {
    if (!drained) {
      grpc_call_cancel(call, nullptr);
      grpc_completion_queue_shutdown(queue);
      while (grpc_completion_queue_next(queue, gpr_inf_future(GPR_CLOCK_REALTIME), nullptr).type != GRPC_QUEUE_SHUTDOWN) {}
    }
    if (buffered) connection->buffered.fetch_sub(buffered);
    if (received) grpc_byte_buffer_destroy(received);
    if (sending) grpc_byte_buffer_destroy(sending);
    grpc_metadata_array_destroy(&initial); grpc_metadata_array_destroy(&trailing);
    grpc_slice_unref(details); grpc_call_unref(call); grpc_completion_queue_destroy(queue);
  }
  grpc_call_error batch(grpc_op *ops, size_t count, intptr_t tag) { return grpc_call_start_batch(call, ops, count, (void *)tag, nullptr); }
  void read() {
    if (closed || reading || awaitingAck) return;
    grpc_op op{}; op.op = GRPC_OP_RECV_MESSAGE; op.data.recv_message.recv_message = &received;
    reading = batch(&op, 1, 2) == GRPC_CALL_OK;
    if (!reading) grpc_call_cancel(call, nullptr);
  }
};
@implementation CanopyGrpc {
  std::mutex _mutex;
  std::map<std::string, std::shared_ptr<Connection>> _connections;
  std::map<std::string, std::shared_ptr<Call>> _calls;
  NSDictionary *_methods;
}
- (instancetype)init {
  if ((self = [super init])) {
    grpc_init();
    NSBundle *bundle = [NSBundle bundleWithURL:[[NSBundle bundleForClass:self.class] URLForResource:@"CanopyRemote" withExtension:@"bundle"]];
    NSData *data = [NSData dataWithContentsOfURL:[bundle URLForResource:@"Methods" withExtension:@"json"]];
    _methods = data ? [NSJSONSerialization JSONObjectWithData:data options:0 error:nil] : @{};
  }
  return self;
}
- (NSDictionary<NSString *, NSString *> *)createIdentity {
  std::unique_ptr<EVP_PKEY_CTX, decltype(&EVP_PKEY_CTX_free)> ctx(EVP_PKEY_CTX_new_id(EVP_PKEY_EC, nullptr), EVP_PKEY_CTX_free);
  EVP_PKEY *rawKey = nullptr;
  if (!ctx || EVP_PKEY_keygen_init(ctx.get()) != 1 || EVP_PKEY_CTX_set_ec_paramgen_curve_nid(ctx.get(), NID_X9_62_prime256v1) != 1 || EVP_PKEY_keygen(ctx.get(), &rawKey) != 1) return @{@"error":@"Device key generation failed"};
  std::unique_ptr<EVP_PKEY, decltype(&EVP_PKEY_free)> key(rawKey, EVP_PKEY_free);
  std::unique_ptr<X509_REQ, decltype(&X509_REQ_free)> request(X509_REQ_new(), X509_REQ_free);
  std::unique_ptr<X509_NAME, decltype(&X509_NAME_free)> name(X509_NAME_new(), X509_NAME_free);
  if (!request || !name || X509_REQ_set_version(request.get(), 0) != 1 || X509_NAME_add_entry_by_txt(name.get(), "CN", MBSTRING_ASC, (const unsigned char *)"Canopy Device", -1, -1, 0) != 1 || X509_REQ_set_subject_name(request.get(), name.get()) != 1 || X509_REQ_set_pubkey(request.get(), key.get()) != 1 || X509_REQ_sign(request.get(), key.get(), EVP_sha256()) <= 0) return @{@"error":@"Device CSR generation failed"};
  std::unique_ptr<BIO, decltype(&BIO_free)> keyOutput(BIO_new(BIO_s_mem()), BIO_free);
  if (!keyOutput || PEM_write_bio_PrivateKey(keyOutput.get(), key.get(), nullptr, nullptr, 0, nullptr, nullptr) != 1) return @{@"error":@"Device key encoding failed"};
  char *keyData = nullptr; long keySize = BIO_get_mem_data(keyOutput.get(), &keyData);
  NSString *privateKey = [[NSString alloc] initWithBytes:keyData length:keySize encoding:NSUTF8StringEncoding];
  int size = i2d_X509_REQ(request.get(), nullptr);
  if (size <= 0 || !privateKey) return @{@"error":@"Device CSR encoding failed"};
  NSMutableData *csr = [NSMutableData dataWithLength:size]; unsigned char *output = (unsigned char *)csr.mutableBytes;
  if (i2d_X509_REQ(request.get(), &output) != size) return @{@"error":@"Device CSR encoding failed"};
  return @{@"privateKey":privateKey, @"csr":[csr base64EncodedStringWithOptions:0]};
}
- (NSString *)connect:(NSString *)identity config:(NSDictionary<NSString *,NSString *> *)config {
  std::lock_guard<std::mutex> lock(_mutex);
  if (_connections.count(identity.UTF8String)) return @"connection already exists";
  auto connection = std::make_shared<Connection>();
  if (config[@"sshAddress"]) {
    connection->ssh = [CanopySsh new];
    NSString *error = [connection->ssh open:config];
    if (error) return error;
  }
  NSData *der = certificateDER(config[@"hostCertificate"]);
  if (!der || fingerprint(der) != config[@"fingerprint"].lowercaseString.UTF8String) return @"host fingerprint mismatch";
  grpc_ssl_verify_peer_options verify{};
  verify.verify_peer_callback = verifyPeer;
  verify.verify_peer_callback_userdata = new std::string(fingerprint(der));
  verify.verify_peer_destruct = [](void *p) { delete static_cast<std::string *>(p); };
  grpc_ssl_pem_key_cert_pair pair = {config[@"privateKey"].UTF8String, config[@"certificateChain"].UTF8String};
  grpc_channel_credentials *credentials = grpc_ssl_credentials_create_ex(config[@"hostCertificate"].UTF8String, pair.private_key ? &pair : nullptr, &verify, nullptr);
  if (!credentials) return @"TLS credentials rejected";
  grpc_arg args[6]{};
  args[0].type = GRPC_ARG_STRING; args[0].key = (char *)"grpc.ssl_target_name_override"; args[0].value.string = (char *)config[@"serverName"].UTF8String;
  args[1].type = GRPC_ARG_INTEGER; args[1].key = (char *)"grpc.max_receive_message_length"; args[1].value.integer = 1024 * 1024;
  args[2].type = GRPC_ARG_INTEGER; args[2].key = (char *)"grpc.enable_retries"; args[2].value.integer = 0;
  args[3].type = GRPC_ARG_STRING; args[3].key = (char *)"grpc.default_authority"; args[3].value.string = (char *)config[@"serverName"].UTF8String;
  args[4].type = GRPC_ARG_INTEGER; args[4].key = (char *)"grpc.keepalive_time_ms"; args[4].value.integer = 30000;
  args[5].type = GRPC_ARG_INTEGER; args[5].key = (char *)"grpc.keepalive_timeout_ms"; args[5].value.integer = 10000;
  grpc_channel_args options = {6, args};
  NSString *address = config[@"address"];
  if ([address containsString:@":"]) address = [NSString stringWithFormat:@"[%@]", address];
  NSString *target = connection->ssh ? [NSString stringWithFormat:@"127.0.0.1:%ld", (long)connection->ssh.port] : [NSString stringWithFormat:@"%@:%@", address, config[@"port"]];
  connection->channel = grpc_channel_create(target.UTF8String, credentials, &options);
  grpc_channel_credentials_release(credentials);
  _connections[identity.UTF8String] = connection;
  return nil;
}
- (NSString *)start:(NSString *)identity callId:(NSString *)callId path:(NSString *)path deadline:(double)milliseconds {
  std::lock_guard<std::mutex> lock(_mutex);
  auto found = _connections.find(identity.UTF8String);
  if (found == _connections.end()) return @"not connected";
  if (_calls.count(callId.UTF8String)) return @"duplicate call";
  NSUInteger count = 0; for (const auto &entry : _calls) if (entry.second->host == identity.UTF8String) ++count;
  if (count >= 32) return @"resource_exhausted";
  NSDictionary *method = nil;
  for (NSDictionary *candidate in _methods.allValues) if ([candidate[@"path"] isEqual:path]) { method = candidate; break; }
  if (!method || milliseconds <= 0 || milliseconds > 3600000) return @"invalid method or deadline";
  auto c = std::make_shared<Call>(); c->host = identity.UTF8String; c->connection = found->second; c->bidi = [method[@"clientStream"] boolValue];
  c->queue = grpc_completion_queue_create_for_next(nullptr);
  grpc_slice name = grpc_slice_from_copied_string(path.UTF8String);
  c->call = grpc_channel_create_call(c->connection->channel, nullptr, 0, c->queue, name, nullptr, gpr_time_add(gpr_now(GPR_CLOCK_REALTIME), gpr_time_from_millis((int64_t)milliseconds, GPR_TIMESPAN)), nullptr);
  grpc_slice_unref(name); grpc_metadata_array_init(&c->initial); grpc_metadata_array_init(&c->trailing);
  grpc_op initial[2]{};
  initial[0].op = GRPC_OP_SEND_INITIAL_METADATA;
  initial[1].op = GRPC_OP_RECV_INITIAL_METADATA; initial[1].data.recv_initial_metadata.recv_initial_metadata = &c->initial;
  if (c->batch(initial, 2, 1) != GRPC_CALL_OK) return @"cannot start RPC";
  grpc_op status{}; status.op = GRPC_OP_RECV_STATUS_ON_CLIENT;
  status.data.recv_status_on_client.trailing_metadata = &c->trailing;
  status.data.recv_status_on_client.status = &c->status; status.data.recv_status_on_client.status_details = &c->details;
  c->batch(&status, 1, 5); c->read(); _calls[callId.UTF8String] = c;
  __weak CanopyGrpc *weakSelf = self;
  dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
    for (;;) {
      grpc_event event = grpc_completion_queue_next(c->queue, gpr_inf_future(GPR_CLOCK_REALTIME), nullptr);
      if (event.type == GRPC_QUEUE_SHUTDOWN) break;
      @autoreleasepool {
        NSDictionary *output = nil;
        {
          std::lock_guard<std::mutex> guard(c->mutex);
          intptr_t tag = (intptr_t)event.tag;
          if (tag == 2) {
            c->reading = false;
            if (event.success && c->received) {
              grpc_byte_buffer_reader reader; grpc_byte_buffer_reader_init(&reader, c->received);
              grpc_slice bytes = grpc_byte_buffer_reader_readall(&reader);
              NSData *data = [NSData dataWithBytes:GRPC_SLICE_START_PTR(bytes) length:GRPC_SLICE_LENGTH(bytes)];
              grpc_slice_unref(bytes); grpc_byte_buffer_reader_destroy(&reader); grpc_byte_buffer_destroy(c->received); c->received = nullptr;
              c->buffered = data.length;
              size_t total = c->connection->buffered.fetch_add(c->buffered) + c->buffered;
              if (total > 4 * 1024 * 1024) {
                grpc_call_cancel_with_status(c->call, GRPC_STATUS_RESOURCE_EXHAUSTED, "resync_required", nullptr);
              } else {
                c->awaitingAck = true;
                output = @{@"id":callId, @"kind":@"message", @"data":[data base64EncodedStringWithOptions:0]};
              }
            } else { c->eof = true; }
          } else if (tag == 3) {
            c->writing = false; grpc_byte_buffer_destroy(c->sending); c->sending = nullptr;
            if (event.success) output = @{@"id":callId, @"kind":@"written"};
          } else if (tag == 5) {
            c->statusReceived = true;
          }
          if (!c->closed && c->statusReceived && (c->eof || c->status != GRPC_STATUS_OK)) {
            c->closed = true;
            output = @{@"id":callId, @"kind":@"close", @"code":@(c->status), @"message":@"RPC completed"};
            grpc_completion_queue_shutdown(c->queue);
          }
        }
        CanopyGrpc *owner = weakSelf;
        if (output && owner.event) owner.event(output);
      }
    }
    c->drained = true;
    CanopyGrpc *owner = weakSelf;
    if (owner) { std::lock_guard<std::mutex> guard(owner->_mutex); owner->_calls.erase(callId.UTF8String); }
  });
  return nil;
}
- (NSString *)write:(NSString *)callId data:(NSString *)base64 {
  std::lock_guard<std::mutex> lock(_mutex);
  auto found = _calls.find(callId.UTF8String); if (found == _calls.end()) return @"call closed";
  auto c = found->second; std::lock_guard<std::mutex> guard(c->mutex);
  if (c->closed || c->ended || c->writing || (!c->bidi && c->sent)) return @"write unavailable";
  if (base64.length > 1398104) return @"resource_exhausted";
  NSData *data = [[NSData alloc] initWithBase64EncodedString:base64 options:0]; if (!data) return @"invalid bytes";
  grpc_slice slice = grpc_slice_from_copied_buffer((const char *)data.bytes, data.length);
  c->sending = grpc_raw_byte_buffer_create(&slice, 1); grpc_slice_unref(slice);
  grpc_op op{}; op.op = GRPC_OP_SEND_MESSAGE; op.data.send_message.send_message = c->sending;
  if (c->batch(&op, 1, 3) != GRPC_CALL_OK) { grpc_byte_buffer_destroy(c->sending); c->sending = nullptr; return @"write rejected"; }
  c->writing = true; c->sent = true; return nil;
}
- (NSString *)ack:(NSString *)callId {
  std::lock_guard<std::mutex> lock(_mutex);
  auto found = _calls.find(callId.UTF8String); if (found == _calls.end()) return nil;
  auto c = found->second; std::lock_guard<std::mutex> guard(c->mutex);
  if (c->buffered) { c->connection->buffered.fetch_sub(c->buffered); c->buffered = 0; }
  c->awaitingAck = false; c->read(); return nil;
}
- (NSString *)finish:(NSString *)callId {
  std::lock_guard<std::mutex> lock(_mutex);
  auto found = _calls.find(callId.UTF8String); if (found == _calls.end()) return @"call closed";
  auto c = found->second; std::lock_guard<std::mutex> guard(c->mutex);
  if (c->writing || c->ended || c->closed) return @"write pending or call ended";
  grpc_op op{}; op.op = GRPC_OP_SEND_CLOSE_FROM_CLIENT;
  if (c->batch(&op, 1, 4) != GRPC_CALL_OK) return @"finish rejected";
  c->ended = true; return nil;
}
- (void)cancel:(NSString *)callId {
  std::lock_guard<std::mutex> lock(_mutex);
  auto found = _calls.find(callId.UTF8String); if (found != _calls.end()) grpc_call_cancel(found->second->call, nullptr);
}
- (void)disconnect:(NSString *)identity {
  std::lock_guard<std::mutex> lock(_mutex);
  for (const auto &entry : _calls) if (entry.second->host == identity.UTF8String) grpc_call_cancel(entry.second->call, nullptr);
  _connections.erase(identity.UTF8String);
}
- (void)shutdown {
  std::lock_guard<std::mutex> lock(_mutex);
  for (const auto &entry : _calls) grpc_call_cancel(entry.second->call, nullptr);
  _connections.clear();
}
- (void)dealloc { [self shutdown]; grpc_shutdown(); }
@end
