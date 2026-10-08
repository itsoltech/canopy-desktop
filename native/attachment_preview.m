#import <AppKit/AppKit.h>
#import <Quartz/Quartz.h>
// All entry points run on the GPUI thread. The callback only queues a message.
typedef void (*CanopyPreviewClose)(void *);
@interface CanopyAttachmentPreview : NSObject
@property QLPreviewView *preview;
@property (weak) NSView *host;
@property id keyMonitor;
@end
@implementation CanopyAttachmentPreview
@end
void *canopy_attachment_create(void *viewPtr,const char *path,void *context,CanopyPreviewClose closeCallback) {
    NSView *host=(__bridge NSView *)viewPtr;
    NSString *file=[[NSString alloc] initWithUTF8String:path];
    if (!host || !file) return NULL;
    CanopyAttachmentPreview *owner=[CanopyAttachmentPreview new];
    owner.host=host;
    owner.preview=[[QLPreviewView alloc] initWithFrame:NSZeroRect style:QLPreviewViewStyleNormal];
    if (!owner.preview) return NULL;
    owner.preview.shouldCloseWithWindow=NO;
    owner.preview.autostarts=NO;
    owner.preview.appearance=[NSAppearance appearanceNamed:NSAppearanceNameDarkAqua];
    owner.preview.hidden=YES;
    owner.preview.previewItem=[NSURL fileURLWithPath:file];
    [host addSubview:owner.preview];
    __weak NSView *weakHost=host;
    owner.keyMonitor=[NSEvent addLocalMonitorForEventsMatchingMask:NSEventMaskKeyDown handler:^NSEvent *(NSEvent *event) {
        NSView *view=weakHost;
        if (view && event.window==view.window && (event.keyCode==53 || ((event.modifierFlags & NSEventModifierFlagCommand) && [[event.charactersIgnoringModifiers lowercaseString] isEqualToString:@"w"]))) {
            closeCallback(context);
            return nil;
        }
        return event;
    }];
    return (__bridge_retained void *)owner;
}
void canopy_attachment_frame(void *ptr,double x,double y,double w,double h,double alpha) {
    CanopyAttachmentPreview *p=(__bridge CanopyAttachmentPreview *)ptr;
    NSView *host=p.host;
    if (!host) return;
    double top=host.isFlipped ? y : host.bounds.size.height-y-h;
    p.preview.frame=NSMakeRect(x,top,MAX(0,w),MAX(0,h));
    p.preview.alphaValue=MIN(1,MAX(0,alpha));
    p.preview.hidden=(w<=0 || h<=0 || alpha<=0);
}
void canopy_attachment_destroy(void *ptr) {
    CanopyAttachmentPreview *p=(__bridge_transfer CanopyAttachmentPreview *)ptr;
    if (p.keyMonitor) [NSEvent removeMonitor:p.keyMonitor];
    p.keyMonitor=nil;
    [p.preview close];
    [p.preview removeFromSuperview];
    p.preview=nil;
}
