#import "CanopySsh.h"
#import <CommonCrypto/CommonDigest.h>
#include <libssh2.h>
#include <arpa/inet.h>
#include <netdb.h>
#include <sys/socket.h>
#include <poll.h>
#include <fcntl.h>
#include <unistd.h>
#include <atomic>
#include <thread>
#include <mutex>
#include <array>
#include <cstring>

@implementation CanopySsh {
  LIBSSH2_SESSION *_session;
  LIBSSH2_CHANNEL *_channel;
  int _socket, _listener;
  std::atomic<bool> _stopped;
  std::thread _worker;
  NSInteger _port;
}
- (instancetype)init {
  if ((self = [super init])) { _socket = _listener = -1; _stopped = false; }
  return self;
}
- (NSInteger)port { return _port; }
- (NSString *)open:(NSDictionary<NSString *,NSString *> *)c {
  static std::once_flag init;
  static int initialized;
  std::call_once(init, [] { initialized = libssh2_init(0); });
  if (initialized) return @"SSH initialization failed";
  NSString *error = [self prepare:c];
  if (error) [self close];
  return error;
}
- (NSString *)prepare:(NSDictionary<NSString *,NSString *> *)c {
  int sshPort = c[@"sshPort"].intValue, remotePort = c[@"port"].intValue;
  if (sshPort < 1 || sshPort > 65535 || remotePort < 1 || remotePort > 65535 || !c[@"sshUsername"].length) return @"Invalid SSH route";
  addrinfo hints{}, *addresses = nullptr; hints.ai_socktype = SOCK_STREAM; hints.ai_family = AF_UNSPEC;
  if (getaddrinfo(c[@"sshAddress"].UTF8String, c[@"sshPort"].UTF8String, &hints, &addresses)) return @"SSH address unavailable";
  for (auto a = addresses; a; a = a->ai_next) {
    _socket = socket(a->ai_family, a->ai_socktype, a->ai_protocol);
    if (_socket < 0) continue;
    fcntl(_socket, F_SETFL, O_NONBLOCK);
    int result = connect(_socket, a->ai_addr, a->ai_addrlen);
    pollfd p{_socket, POLLOUT, 0}; int socketError = 0; socklen_t size = sizeof(socketError);
    if ((result == 0 || (errno == EINPROGRESS && poll(&p, 1, 15000) > 0)) && getsockopt(_socket, SOL_SOCKET, SO_ERROR, &socketError, &size) == 0 && !socketError) break;
    ::close(_socket); _socket = -1;
  }
  freeaddrinfo(addresses);
  if (_socket < 0) return @"SSH connection failed";
  int one = 1; setsockopt(_socket, SOL_SOCKET, SO_NOSIGPIPE, &one, sizeof(one));
  fcntl(_socket, F_SETFL, 0);
  _session = libssh2_session_init();
  if (!_session) return @"SSH session allocation failed";
  libssh2_session_set_timeout(_session, 15000);
  if (libssh2_session_handshake(_session, _socket)) return @"SSH handshake failed";
  size_t keyLength = 0; int keyType = 0;
  const char *key = libssh2_session_hostkey(_session, &keyLength, &keyType);
  if (!key) return @"SSH host key unavailable";
  unsigned char digest[CC_SHA256_DIGEST_LENGTH]; CC_SHA256(key, (CC_LONG)keyLength, digest);
  NSString *pin = [@"SHA256:" stringByAppendingString:[[[NSData dataWithBytes:digest length:sizeof(digest)] base64EncodedStringWithOptions:0] stringByReplacingOccurrencesOfString:@"=" withString:@""]];
  if (![pin isEqualToString:c[@"sshFingerprint"]]) return @"SSH host key changed; verify the fingerprint";
  NSData *user = [c[@"sshUsername"] dataUsingEncoding:NSUTF8StringEncoding];
  int auth = -1;
  if (c[@"sshPrivateKey"].length) {
    NSData *privateKey = [c[@"sshPrivateKey"] dataUsingEncoding:NSUTF8StringEncoding];
    auth = libssh2_userauth_publickey_frommemory(_session, (const char *)user.bytes, user.length, nullptr, 0, (const char *)privateKey.bytes, privateKey.length, c[@"sshPassphrase"].UTF8String);
  } else if (c[@"sshPassword"]) {
    NSData *password = [c[@"sshPassword"] dataUsingEncoding:NSUTF8StringEncoding];
    auth = libssh2_userauth_password_ex(_session, (const char *)user.bytes, (unsigned int)user.length, (const char *)password.bytes, (unsigned int)password.length, nullptr);
  }
  if (auth) return @"SSH authentication failed";
  _channel = libssh2_channel_direct_tcpip_ex(_session, c[@"address"].UTF8String, remotePort, "127.0.0.1", 0);
  if (!_channel) return @"SSH forwarding refused";
  _listener = socket(AF_INET, SOCK_STREAM, 0);
  sockaddr_in address{}; address.sin_family = AF_INET; address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
  if (_listener < 0 || bind(_listener, (sockaddr *)&address, sizeof(address)) || listen(_listener, 1)) return @"SSH loopback listener failed";
  socklen_t length = sizeof(address);
  if (getsockname(_listener, (sockaddr *)&address, &length)) return @"SSH loopback address failed";
  _port = ntohs(address.sin_port);
  fcntl(_listener, F_SETFL, O_NONBLOCK); fcntl(_socket, F_SETFL, O_NONBLOCK);
  libssh2_session_set_blocking(_session, 0);
  // A single worker serializes libssh2 access, with bounded buffers in each direction.
  _worker = std::thread([self] { [self forward]; });
  return nil;
}
- (void)forward {
  @autoreleasepool {
    int peer = -1;
    while (!_stopped && peer < 0) { pollfd p{_listener, POLLIN, 0}; if (poll(&p, 1, 100) > 0) peer = accept(_listener, nullptr, nullptr); }
    if (peer < 0) return;
    int one = 1; setsockopt(peer, SOL_SOCKET, SO_NOSIGPIPE, &one, sizeof(one)); fcntl(peer, F_SETFL, O_NONBLOCK);
    std::array<char, 32768> up{}, down{}; size_t upSize = 0, upOffset = 0, downSize = 0, downOffset = 0;
    while (!_stopped) {
      if (upSize == upOffset) {
        auto n = recv(peer, up.data(), up.size(), 0);
        if (n == 0 || (n < 0 && errno != EAGAIN && errno != EWOULDBLOCK)) break;
        if (n > 0) { upSize = n; upOffset = 0; }
      }
      if (upOffset < upSize) {
        auto n = libssh2_channel_write(_channel, up.data() + upOffset, upSize - upOffset);
        if (n < 0 && n != LIBSSH2_ERROR_EAGAIN) break;
        if (n > 0) upOffset += n;
      }
      if (downSize == downOffset) {
        auto n = libssh2_channel_read(_channel, down.data(), down.size());
        if ((n < 0 && n != LIBSSH2_ERROR_EAGAIN) || (n == 0 && libssh2_channel_eof(_channel))) break;
        if (n > 0) { downSize = n; downOffset = 0; }
      }
      if (downOffset < downSize) {
        auto n = send(peer, down.data() + downOffset, downSize - downOffset, 0);
        if (n < 0 && errno != EAGAIN && errno != EWOULDBLOCK) break;
        if (n > 0) downOffset += n;
      }
      int directions = libssh2_session_block_directions(_session);
      pollfd wait[2] = {{peer, (short)((upOffset == upSize ? POLLIN : 0) | (downOffset < downSize ? POLLOUT : 0)), 0}, {_socket, (short)(((directions & LIBSSH2_SESSION_BLOCK_INBOUND) ? POLLIN : 0) | ((directions & LIBSSH2_SESSION_BLOCK_OUTBOUND) ? POLLOUT : 0)), 0}};
      if (poll(wait, 2, 100) < 0 && errno != EINTR) break;
    }
    shutdown(peer, SHUT_RDWR); ::close(peer);
  }
}
- (void)close {
  _stopped = true;
  if (_worker.joinable()) _worker.join();
  if (_socket >= 0) shutdown(_socket, SHUT_RDWR);
  if (_session) { libssh2_session_set_blocking(_session, 1); libssh2_session_set_timeout(_session, 1000); }
  if (_channel) { libssh2_channel_free(_channel); _channel = nullptr; }
  if (_session) { libssh2_session_free(_session); _session = nullptr; }
  if (_socket >= 0) { ::close(_socket); _socket = -1; }
  if (_listener >= 0) { ::close(_listener); _listener = -1; }
}
- (void)dealloc { [self close]; }
@end
