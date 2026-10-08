#import <AppKit/AppKit.h>
#import <AVFoundation/AVFoundation.h>
#import <QuartzCore/QuartzCore.h>

// All entry points are called on the GPUI UI thread. The bridge owns only its
// player/layer, never the application window or the user's media file.
@interface CanopyVideoPreview : NSObject
@property AVPlayer *player;
@property AVPlayerLayer *layer;
@property (weak) NSView *view;
@end
@implementation CanopyVideoPreview
@end
void *canopy_video_create(void *viewPtr, const char *path) {
    NSView *view = (__bridge NSView *)viewPtr;
    NSString *file = [[NSString alloc] initWithUTF8String:path];
    if (!file || !view.layer) return NULL;
    CanopyVideoPreview *preview = [CanopyVideoPreview new];
    preview.view = view;
    preview.player = [AVPlayer playerWithURL:[NSURL fileURLWithPath:file]];
    preview.layer = [AVPlayerLayer playerLayerWithPlayer:preview.player];
    preview.layer.videoGravity = AVLayerVideoGravityResizeAspect;
    preview.layer.hidden = YES;
    [view.layer addSublayer:preview.layer];
    return (__bridge_retained void *)preview;
}
void canopy_video_frame(void *ptr, double x, double y, double w, double h) {
    CanopyVideoPreview *p = (__bridge CanopyVideoPreview *)ptr;
    NSView *view=p.view;
    [CATransaction begin]; [CATransaction setDisableActions:YES];
    // CALayer uses a bottom-left origin unless its host layer is flipped.
    double top = view.layer.geometryFlipped ? y : view.bounds.size.height-y-h;
    p.layer.frame=CGRectMake(x,top,MAX(0,w),MAX(0,h));p.layer.hidden=NO;
    [CATransaction commit];
}
void canopy_video_hide(void *ptr) {
    CanopyVideoPreview *p=(__bridge CanopyVideoPreview *)ptr;
    [p.player pause];p.layer.hidden=YES;
}
void canopy_video_play(void *ptr, bool play) {
    CanopyVideoPreview *p=(__bridge CanopyVideoPreview *)ptr;
    if(play) [p.player play];else [p.player pause];
}
void canopy_video_seek(void *ptr,double seconds) {
    CanopyVideoPreview *p=(__bridge CanopyVideoPreview *)ptr;
    [p.player seekToTime:CMTimeMakeWithSeconds(MAX(0,seconds),600) toleranceBefore:kCMTimeZero toleranceAfter:kCMTimeZero];
}
int canopy_video_status(void *ptr,double *time,double *duration,bool *playing) {
    CanopyVideoPreview *p=(__bridge CanopyVideoPreview *)ptr;
    *time=CMTimeGetSeconds(p.player.currentTime);*duration=CMTimeGetSeconds(p.player.currentItem.duration);*playing=p.player.rate!=0;
    return (int)p.player.currentItem.status;
}
void canopy_video_destroy(void *ptr) {
    CanopyVideoPreview *p=(__bridge_transfer CanopyVideoPreview *)ptr;
    [p.player pause];[p.layer removeFromSuperlayer];p.layer.player=nil;p.player=nil;
}
