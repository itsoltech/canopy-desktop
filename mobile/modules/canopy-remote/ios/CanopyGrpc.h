#import <Foundation/Foundation.h>
NS_ASSUME_NONNULL_BEGIN
@interface CanopyGrpc : NSObject
@property(nonatomic, copy) void (^event)(NSDictionary *);
- (NSDictionary<NSString *, NSString *> *)createIdentity;
- (nullable NSString *)connect:(NSString *)identity config:(NSDictionary<NSString *, NSString *> *)config;
- (nullable NSString *)start:(NSString *)identity callId:(NSString *)callId path:(NSString *)path deadline:(double)milliseconds;
- (nullable NSString *)write:(NSString *)callId data:(NSString *)base64;
- (nullable NSString *)ack:(NSString *)callId;
- (nullable NSString *)finish:(NSString *)callId;
- (void)cancel:(NSString *)callId;
- (void)disconnect:(NSString *)identity;
- (void)shutdown;
@end
NS_ASSUME_NONNULL_END
