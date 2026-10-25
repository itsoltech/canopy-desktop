#import <Foundation/Foundation.h>
// Owns the socket, SSH session and forwarding worker. No shell/known_hosts.
@interface CanopySsh : NSObject
@property(nonatomic, readonly) NSInteger port;
- (nullable NSString *)open:(NSDictionary<NSString *, NSString *> *)config;
- (void)close;
@end
