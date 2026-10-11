#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>
#import <WebKit/WebKit.h>

// Pages an agent shows in the chat (WKWebView). Every entry point runs on the GPUI
// main thread. A page is isolated: a non-persistent data store, no file access
// (no base URL), links and new windows go to the default browser, no camera or
// microphone, no downloads. Messages from the page (content height, console) go to
// a C callback that only forwards them to Rust.

typedef void (*CanopyPageEvent)(void *ctx, int kind, const char *text, double value);
typedef void (*CanopyPageImage)(void *ctx, const unsigned char *bytes, size_t length,
                                double height);

static NSString *const CanopyBridge = @"canopy";
// The page's own origin: a real one, so its errors aren't hidden as cross-origin, at a
// host that never resolves; `file:` stays out of reach of an https page.
static NSString *const CanopyBase = @"https://page.invalid/";

// Reports the content height and console lines; runs before the page's own scripts.
static NSString *CanopyScript(void) {
    return @"(function(){"
            "const post=(m)=>{try{window.webkit.messageHandlers.canopy.postMessage(m)}catch(e){}};"
            "const text=(a)=>a.map(x=>{if(x instanceof Error)return x.stack||String(x);"
            "if(typeof x==='object'){try{return JSON.stringify(x)}catch(e){}}return String(x)}).join(' ');"
            "for(const level of ['log','info','warn','error','debug']){const original=console[level];"
            "console[level]=function(...a){post({kind:'console',text:level+': '+text(a)});"
            "return original&&original.apply(console,a)}}"
            "window.addEventListener('error',e=>post({kind:'console',text:'error: '+(e.message||'Error')+"
            "(e.lineno?' (line '+e.lineno+':'+e.colno+')':'')+(e.error&&e.error.stack?'\\n'+e.error.stack:'')}));"
            "window.addEventListener('unhandledrejection',e=>post({kind:'console',"
            "text:'error: unhandled rejection: '+text([e.reason])}));"
            // The body's own height: the document's is never less than the view, so it
            // couldn't tell a short page from a tall view.
            "let last=-1;const measure=()=>{const b=document.body;if(!b)return 0;"
            "const s=getComputedStyle(b);return Math.ceil(b.getBoundingClientRect().height+"
            "parseFloat(s.marginTop)+parseFloat(s.marginBottom))};"
            "const height=()=>{const h=measure();if(h!==last){last=h;post({kind:'height',value:h})}};"
            "window.__canopyHeight=measure;"
            "const watch=()=>{height();try{const o=new ResizeObserver(height);"
            "o.observe(document.documentElement);if(document.body)o.observe(document.body)}catch(e){}};"
            "if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',watch);"
            "else watch();window.addEventListener('load',height);"
            "if(document.fonts)document.fonts.ready.then(height);"
            "})();";
}

static void CanopyOpenExternally(NSURL *url) {
    NSString *scheme = url.scheme.lowercaseString;
    if ([scheme isEqualToString:@"http"] || [scheme isEqualToString:@"https"] ||
        [scheme isEqualToString:@"mailto"]) {
        [[NSWorkspace sharedWorkspace] openURL:url];
    }
}

// Scrolling over a page that has nothing to scroll moves the conversation instead.
@interface CanopyPageWebView : WKWebView
@property(weak) NSView *host;
@property BOOL scrollsItself;
@end
@implementation CanopyPageWebView
- (void)scrollWheel:(NSEvent *)event {
    if (!self.scrollsItself && self.host) {
        [self.host scrollWheel:event];
    } else {
        [super scrollWheel:event];
    }
}
@end

// Holds a page cut to the conversation. While the page is only warming up it is fully
// transparent and lets every click and scroll through to Canopy beneath.
@interface CanopyFlippedView : NSView
@property BOOL active;
@end
@implementation CanopyFlippedView
- (BOOL)isFlipped {
    return YES;
}
- (NSView *)hitTest:(NSPoint)point {
    return self.active ? [super hitTest:point] : nil;
}
@end

@class CanopyPage;
// The content controller retains its handler; this one holds the page weakly.
@interface CanopyPageMessages : NSObject <WKScriptMessageHandler>
@property(weak) CanopyPage *page;
@end

@interface CanopyPage : NSObject <WKNavigationDelegate, WKUIDelegate>
@property NSView *clip;
@property CanopyPageWebView *web;
@property NSWindow *offscreen;
@property(weak) NSView *host;
@property void *ctx;
@property CanopyPageEvent onEvent;
@property CanopyPageImage onImage;
@property double width;
@property double maxHeight;
/// Pixels per point of the snapshot.
@property double scale;
@property BOOL loaded;
@property BOOL capturing;
@end

@implementation CanopyPageMessages
- (void)userContentController:(WKUserContentController *)controller
      didReceiveScriptMessage:(WKScriptMessage *)message {
    CanopyPage *page = self.page;
    if (!page || !page.onEvent || ![message.body isKindOfClass:[NSDictionary class]]) return;
    NSDictionary *body = message.body;
    NSString *kind = body[@"kind"];
    if ([kind isEqualToString:@"height"] && [body[@"value"] isKindOfClass:[NSNumber class]]) {
        double value = [body[@"value"] doubleValue];
        page.web.scrollsItself = value > page.maxHeight;
        page.onEvent(page.ctx, 0, NULL, value);
    } else if ([kind isEqualToString:@"console"] && [body[@"text"] isKindOfClass:[NSString class]]) {
        NSString *text = body[@"text"];
        if (text.length > 4000) text = [text substringToIndex:4000];
        page.onEvent(page.ctx, 1, text.UTF8String, 0);
    }
}
@end

@implementation CanopyPage
- (void)webView:(WKWebView *)webView
    decidePolicyForNavigationAction:(WKNavigationAction *)action
                    decisionHandler:(void (^)(WKNavigationActionPolicy))handler {
    NSURL *url = action.request.URL;
    BOOL main = action.targetFrame == nil || action.targetFrame.isMainFrame;
    // The page's own document loads; anything else in the main frame leaves for the browser.
    if (main && (action.navigationType == WKNavigationTypeLinkActivated ||
                 action.targetFrame == nil || self.loaded)) {
        CanopyOpenExternally(url);
        handler(WKNavigationActionPolicyCancel);
        return;
    }
    NSString *scheme = url.scheme.lowercaseString;
    if (!main && !([scheme isEqualToString:@"http"] || [scheme isEqualToString:@"https"] ||
                   [scheme isEqualToString:@"about"] || [scheme isEqualToString:@"data"] ||
                   [scheme isEqualToString:@"blob"])) {
        handler(WKNavigationActionPolicyCancel);
        return;
    }
    handler(WKNavigationActionPolicyAllow);
}
- (void)webView:(WKWebView *)webView
    decidePolicyForNavigationResponse:(WKNavigationResponse *)response
                      decisionHandler:(void (^)(WKNavigationResponsePolicy))handler {
    handler(response.canShowMIMEType ? WKNavigationResponsePolicyAllow
                                     : WKNavigationResponsePolicyCancel);
}
- (void)webView:(WKWebView *)webView didFinishNavigation:(WKNavigation *)navigation {
    self.loaded = YES;
    if (self.capturing) [self captureAfterFonts];
}
- (void)webView:(WKWebView *)webView
    didFailProvisionalNavigation:(WKNavigation *)navigation
                       withError:(NSError *)error {
    if (self.onEvent) self.onEvent(self.ctx, 1, error.localizedDescription.UTF8String, 0);
    if (self.capturing) [self captureAfterFonts];
}
- (WKWebView *)webView:(WKWebView *)webView
    createWebViewWithConfiguration:(WKWebViewConfiguration *)configuration
               forNavigationAction:(WKNavigationAction *)action
                    windowFeatures:(WKWindowFeatures *)features {
    CanopyOpenExternally(action.request.URL);
    return nil;
}
- (void)webView:(WKWebView *)webView
    runJavaScriptAlertPanelWithMessage:(NSString *)message
                      initiatedByFrame:(WKFrameInfo *)frame
                     completionHandler:(void (^)(void))handler {
    handler();
}
- (void)webView:(WKWebView *)webView
    runJavaScriptConfirmPanelWithMessage:(NSString *)message
                        initiatedByFrame:(WKFrameInfo *)frame
                       completionHandler:(void (^)(BOOL))handler {
    handler(NO);
}
- (void)webView:(WKWebView *)webView
    requestMediaCapturePermissionForOrigin:(WKSecurityOrigin *)origin
                          initiatedByFrame:(WKFrameInfo *)frame
                                      type:(WKMediaCaptureType)type
                           decisionHandler:(void (^)(WKPermissionDecision))handler
    API_AVAILABLE(macos(12.0)) {
    handler(WKPermissionDecisionDeny);
}

// Waits for fonts and a frame, sizes the view to the content, then snapshots it.
- (void)captureAfterFonts {
    if (!self.capturing) return;
    __weak CanopyPage *weakSelf = self;
    // A timer, not a frame callback: WebKit holds frames back in a window nobody sees.
    // After fonts and the page's own animations (at most 1.5 s, for endless ones): the
    // snapshot then matches the page as it is once shown.
    NSString *measure = @"(document.fonts?document.fonts.ready:Promise.resolve()).then(()=>"
                         "Promise.race([Promise.all((document.getAnimations?document.getAnimations():[])"
                         ".map(a=>a.finished.catch(()=>{}))),new Promise(r=>setTimeout(r,1500))]))"
                         ".then(()=>new Promise(r=>setTimeout(()=>r(window.__canopyHeight?"
                         "window.__canopyHeight():document.documentElement.scrollHeight),60)))";
    [self.web callAsyncJavaScript:[@"return " stringByAppendingString:measure]
                        arguments:@{}
                          inFrame:nil
                   inContentWorld:WKContentWorld.pageWorld
                completionHandler:^(id value, __unused NSError *error) {
                    CanopyPage *page = weakSelf;
                    if (!page) return;
                    double height = [value isKindOfClass:[NSNumber class]] ? [value doubleValue] : 0;
                    double shown = MIN(MAX(height, 1), page.maxHeight);
                    NSRect frame = NSMakeRect(0, 0, page.width, shown);
                    [page.offscreen setContentSize:frame.size];
                    page.web.frame = frame;
                    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 120 * NSEC_PER_MSEC),
                                   dispatch_get_main_queue(), ^{
                                       [page snapshotWithHeight:height];
                                   });
                }];
}

- (void)snapshotWithHeight:(double)height {
    __weak CanopyPage *weakSelf = self;
    WKSnapshotConfiguration *config = [WKSnapshotConfiguration new];
    // The width is in points of this window: its own scale times this gives the pixels
    // asked for, so the snapshot is as sharp as the chat window draws it.
    CGFloat backing = self.offscreen.backingScaleFactor ?: 1;
    config.snapshotWidth = @(self.web.frame.size.width * MAX(self.scale, 1) / backing);
    [self.web takeSnapshotWithConfiguration:config
                          completionHandler:^(NSImage *image, __unused NSError *error) {
                              CanopyPage *page = weakSelf;
                              if (!page || !page.onImage) return;
                              page.capturing = NO;
                              NSData *png = nil;
                              CGImageRef cg = [image CGImageForProposedRect:NULL
                                                                    context:nil
                                                                      hints:nil];
                              if (cg) {
                                  NSBitmapImageRep *rep =
                                      [[NSBitmapImageRep alloc] initWithCGImage:cg];
                                  png = [rep representationUsingType:NSBitmapImageFileTypePNG
                                                          properties:@{}];
                              }
                              page.onImage(page.ctx, png.bytes, png.length, height);
                          }];
}
@end

static CanopyPage *CanopyMakePage(NSString *html, NSString *css, double width, double maxHeight,
                                  void *ctx, CanopyPageEvent onEvent, CanopyPageImage onImage) {
    CanopyPage *page = [CanopyPage new];
    page.ctx = ctx;
    page.onEvent = onEvent;
    page.onImage = onImage;
    page.width = width;
    page.maxHeight = maxHeight;
    WKWebViewConfiguration *config = [WKWebViewConfiguration new];
    config.websiteDataStore = [WKWebsiteDataStore nonPersistentDataStore];
    WKUserContentController *content = [WKUserContentController new];
    CanopyPageMessages *messages = [CanopyPageMessages new];
    messages.page = page;
    [content addScriptMessageHandler:messages name:CanopyBridge];
    // The theme goes first in <head>, so the page's own styles win over it.
    NSData *literal = [NSJSONSerialization dataWithJSONObject:css
                                                      options:NSJSONWritingFragmentsAllowed
                                                        error:nil];
    NSString *style = [NSString stringWithFormat:
        @"(function(){const s=document.createElement('style');s.textContent=%@;"
         "(document.head||document.documentElement).prepend(s);})();",
        [[NSString alloc] initWithData:literal encoding:NSUTF8StringEncoding] ?: @"''"];
    [content addUserScript:[[WKUserScript alloc] initWithSource:CanopyScript()
                                                  injectionTime:WKUserScriptInjectionTimeAtDocumentStart
                                               forMainFrameOnly:YES]];
    [content addUserScript:[[WKUserScript alloc] initWithSource:style
                                                  injectionTime:WKUserScriptInjectionTimeAtDocumentEnd
                                               forMainFrameOnly:YES]];
    config.userContentController = content;
    page.web = [[CanopyPageWebView alloc] initWithFrame:NSMakeRect(0, 0, width, 100)
                                          configuration:config];
    [page.web setValue:@NO forKey:@"drawsBackground"];
    page.web.navigationDelegate = page;
    page.web.UIDelegate = page;
    [page.web loadHTMLString:html baseURL:[NSURL URLWithString:CanopyBase]];
    return page;
}

void *canopy_page_create(void *viewPtr, const char *html, const char *css, double maxHeight,
                         void *ctx, CanopyPageEvent onEvent) {
    NSView *view = (__bridge NSView *)viewPtr;
    NSString *source = [[NSString alloc] initWithUTF8String:html];
    NSString *style = [[NSString alloc] initWithUTF8String:css];
    if (!view || !source || !style) return NULL;
    CanopyPage *page = CanopyMakePage(source, style, 100, maxHeight, ctx, onEvent, NULL);
    page.host = view;
    page.web.host = view;
    page.clip = [[CanopyFlippedView alloc] initWithFrame:NSZeroRect];
    page.clip.alphaValue = 0;
    page.clip.wantsLayer = YES;
    page.clip.layer.masksToBounds = YES;
    page.clip.hidden = YES;
    [page.clip addSubview:page.web];
    [view addSubview:page.clip];
    return (__bridge_retained void *)page;
}

// `clip` and `rect` are in the host view's top-left coordinates.
void canopy_page_frame(void *ptr, double cx, double cy, double cw, double ch, double x, double y,
                       double w, double h) {
    CanopyPage *page = (__bridge CanopyPage *)ptr;
    NSView *host = page.host;
    if (!host) return;
    double top = host.isFlipped ? cy : host.bounds.size.height - cy - ch;
    [CATransaction begin];
    [CATransaction setDisableActions:YES];
    page.clip.frame = NSMakeRect(cx, top, MAX(0, cw), MAX(0, ch));
    page.web.frame = NSMakeRect(x - cx, y - cy, MAX(1, w), MAX(1, h));
    page.clip.hidden = NO;
    [CATransaction commit];
}

// Keys typed after the pointer left go to Canopy again, not to a page that went away.
static void CanopyReleaseKeys(CanopyPage *page) {
    NSResponder *responder = page.web.window.firstResponder;
    if ([responder isKindOfClass:[NSView class]] && [(NSView *)responder isDescendantOf:page.clip] &&
        page.host) {
        [page.web.window makeFirstResponder:page.host];
    }
}

// Shown and taking the pointer, or warming up: laid out and running (so its animations
// have played when it is shown), but transparent and out of the way.
void canopy_page_set_active(void *ptr, bool active) {
    CanopyPage *page = (__bridge CanopyPage *)ptr;
    CanopyFlippedView *clip = (CanopyFlippedView *)page.clip;
    if (clip.active == active) return;
    if (!active) CanopyReleaseKeys(page);
    clip.active = active;
    [CATransaction begin];
    [CATransaction setDisableActions:YES];
    clip.alphaValue = active ? 1 : 0;
    [CATransaction commit];
}

void canopy_page_hide(void *ptr) {
    CanopyPage *page = (__bridge CanopyPage *)ptr;
    canopy_page_set_active(ptr, false);
    if (page.clip.hidden) return;
    page.clip.hidden = YES;
}

void canopy_page_destroy(void *ptr) {
    CanopyPage *page = (__bridge_transfer CanopyPage *)ptr;
    page.onEvent = NULL;
    page.onImage = NULL;
    [page.web stopLoading];
    [page.web.configuration.userContentController removeScriptMessageHandlerForName:CanopyBridge];
    page.web.navigationDelegate = nil;
    page.web.UIDelegate = nil;
    [page.clip removeFromSuperview];
    [page.offscreen close];
    page.offscreen = nil;
}

// Renders `html` at `width` in a window nobody sees: `onImage` gets the PNG of the
// page (at most `maxHeight` tall) and its full content height; `onEvent` the console.
// The caller destroys the page afterwards with `canopy_page_destroy`.
void *canopy_page_capture(const char *html, const char *css, double width, double maxHeight,
                          double scale, void *ctx, CanopyPageEvent onEvent,
                          CanopyPageImage onImage) {
    NSString *source = [[NSString alloc] initWithUTF8String:html];
    NSString *style = [[NSString alloc] initWithUTF8String:css];
    if (!source || !style) return NULL;
    CanopyPage *page = CanopyMakePage(source, style, width, maxHeight, ctx, onEvent, onImage);
    page.capturing = YES;
    page.scale = scale;
    // On the main screen for its Retina scale, but fully transparent, behind every
    // window and never clickable: nobody sees it, and WebKit still paints it.
    NSScreen *screen = NSScreen.mainScreen ?: NSScreen.screens.firstObject;
    NSPoint origin = screen ? screen.frame.origin : NSZeroPoint;
    NSWindow *window = [[NSWindow alloc] initWithContentRect:NSMakeRect(origin.x, origin.y, width, 100)
                                                   styleMask:NSWindowStyleMaskBorderless
                                                     backing:NSBackingStoreBuffered
                                                       defer:NO
                                                      screen:screen];
    window.releasedWhenClosed = NO;
    window.ignoresMouseEvents = YES;
    window.hasShadow = NO;
    window.opaque = NO;
    window.alphaValue = 0;
    window.backgroundColor = NSColor.clearColor;
    window.excludedFromWindowsMenu = YES;
    window.level = NSNormalWindowLevel - 1;
    window.collectionBehavior = NSWindowCollectionBehaviorTransient |
                                NSWindowCollectionBehaviorIgnoresCycle |
                                NSWindowCollectionBehaviorStationary;
    [window orderBack:nil];
    page.offscreen = window;
    page.web.frame = NSMakeRect(0, 0, width, 100);
    [window.contentView addSubview:page.web];
    return (__bridge_retained void *)page;
}
