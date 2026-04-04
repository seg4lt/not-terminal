#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

#import <AppKit/AppKit.h>
#import <WebKit/WebKit.h>

// WebView wrapper structure
typedef struct rust_webview_s {
    WKWebView *webview;
    NSView *container;
} rust_webview_t;

// Custom view class that refuses first responder to prevent keyboard capture
@interface NonFirstResponderNSView : NSView
@end
@implementation NonFirstResponderNSView
- (BOOL)acceptsFirstResponder {
    return NO;
}
@end

@interface NonFirstResponderWKWebView : WKWebView
@end
@implementation NonFirstResponderWKWebView
- (BOOL)acceptsFirstResponder {
    return NO;
}
- (BOOL)becomeFirstResponder {
    return NO;
}
@end

// Create a new webview hosted in a container view
void *webview_new(void *parent_ns_view) {
    if (parent_ns_view == NULL) {
        return NULL;
    }

    NSView *parent = (NSView *)parent_ns_view;
    NSRect frame = parent.bounds;

    // Create container to hold the webview (uses custom class that refuses first responder)
    NSView *container = [[NonFirstResponderNSView alloc] initWithFrame:frame];
    if (container == nil) {
        return NULL;
    }

    [container setHidden:YES];
    [container setAutoresizingMask:NSViewWidthSizable | NSViewHeightSizable];
    [parent addSubview:container];

    // Create WKWebView configuration
    WKWebViewConfiguration *config = [[WKWebViewConfiguration alloc] init];
    if (config == nil) {
        [container release];
        return NULL;
    }

    // Create the WKWebView
    WKWebView *webview = [[NonFirstResponderWKWebView alloc] initWithFrame:frame configuration:config];
    if (webview == nil) {
        [config release];
        [container release];
        return NULL;
    }

    [webview setAutoresizingMask:NSViewWidthSizable | NSViewHeightSizable];
    [webview setNavigationDelegate:nil];
    [container addSubview:webview];

    // Allocate and return the wrapper
    rust_webview_t *wrapper = (rust_webview_t *)malloc(sizeof(rust_webview_t));
    if (wrapper == NULL) {
        [webview release];
        [container release];
        return NULL;
    }

    wrapper->webview = webview;
    wrapper->container = container;

    return (void *)wrapper;
}

// Free the webview and its resources
void webview_free(void *webview_ptr) {
    if (webview_ptr == NULL) {
        return;
    }

    rust_webview_t *wrapper = (rust_webview_t *)webview_ptr;

    if (wrapper->webview != nil) {
        [wrapper->webview removeFromSuperview];
        [wrapper->webview release];
        wrapper->webview = nil;
    }

    if (wrapper->container != nil) {
        [wrapper->container removeFromSuperview];
        [wrapper->container release];
        wrapper->container = nil;
    }

    free(wrapper);
}

// Load a URL in the webview
void webview_load_url(void *webview_ptr, const char *url_cstr) {
    if (webview_ptr == NULL || url_cstr == NULL) {
        return;
    }

    rust_webview_t *wrapper = (rust_webview_t *)webview_ptr;
    if (wrapper->webview == nil) {
        return;
    }

    NSString *urlString = [NSString stringWithUTF8String:url_cstr];
    if (urlString == nil) {
        return;
    }

    // Add https:// if no scheme is present
    if (![urlString containsString:@"://"]) {
        urlString = [@"https://" stringByAppendingString:urlString];
    }

    NSURL *url = [NSURL URLWithString:urlString];
    if (url == nil) {
        return;
    }

    NSURLRequest *request = [NSURLRequest requestWithURL:url];
    [wrapper->webview loadRequest:request];
}

void webview_load_html(void *webview_ptr, const char *html_cstr) {
    if (webview_ptr == NULL || html_cstr == NULL) {
        return;
    }

    rust_webview_t *wrapper = (rust_webview_t *)webview_ptr;
    if (wrapper->webview == nil) {
        return;
    }

    NSString *html = [NSString stringWithUTF8String:html_cstr];
    if (html == nil) {
        return;
    }

    [wrapper->webview loadHTMLString:html baseURL:nil];
}

// Navigate back in history
void webview_go_back(void *webview_ptr) {
    if (webview_ptr == NULL) {
        return;
    }

    rust_webview_t *wrapper = (rust_webview_t *)webview_ptr;
    if (wrapper->webview == nil) {
        return;
    }

    [wrapper->webview goBack];
}

// Navigate forward in history
void webview_go_forward(void *webview_ptr) {
    if (webview_ptr == NULL) {
        return;
    }

    rust_webview_t *wrapper = (rust_webview_t *)webview_ptr;
    if (wrapper->webview == nil) {
        return;
    }

    [wrapper->webview goForward];
}

// Reload the current page
void webview_reload(void *webview_ptr) {
    if (webview_ptr == NULL) {
        return;
    }

    rust_webview_t *wrapper = (rust_webview_t *)webview_ptr;
    if (wrapper->webview == nil) {
        return;
    }

    [wrapper->webview reload];
}

// Update the webview's frame/size
void webview_set_frame(void *webview_ptr, double x, double y, double width, double height) {
    if (webview_ptr == NULL) {
        return;
    }

    rust_webview_t *wrapper = (rust_webview_t *)webview_ptr;
    if (wrapper->container == nil) {
        return;
    }

    NSRect frame = NSMakeRect((CGFloat)x,
                              (CGFloat)y,
                              (CGFloat)(width < 1.0 ? 1.0 : width),
                              (CGFloat)(height < 1.0 ? 1.0 : height));
    [wrapper->container setFrame:frame];
}

// Set container visibility
void webview_set_hidden(void *webview_ptr, bool hidden) {
    if (webview_ptr == NULL) {
        return;
    }

    rust_webview_t *wrapper = (rust_webview_t *)webview_ptr;
    if (wrapper->container == nil) {
        return;
    }

    [wrapper->container setHidden:hidden ? YES : NO];
}

// Check if can go back
bool webview_can_go_back(void *webview_ptr) {
    if (webview_ptr == NULL) {
        return false;
    }

    rust_webview_t *wrapper = (rust_webview_t *)webview_ptr;
    if (wrapper->webview == nil) {
        return false;
    }

    return [wrapper->webview canGoBack];
}

// Check if can go forward
bool webview_can_go_forward(void *webview_ptr) {
    if (webview_ptr == NULL) {
        return false;
    }

    rust_webview_t *wrapper = (rust_webview_t *)webview_ptr;
    if (wrapper->webview == nil) {
        return false;
    }

    return [wrapper->webview canGoForward];
}

// Get current URL (caller must free the result)
char *webview_get_url(void *webview_ptr) {
    if (webview_ptr == NULL) {
        return NULL;
    }

    rust_webview_t *wrapper = (rust_webview_t *)webview_ptr;
    if (wrapper->webview == nil) {
        return NULL;
    }

    NSURL *url = [wrapper->webview URL];
    if (url == nil) {
        return NULL;
    }

    NSString *urlString = [url absoluteString];
    if (urlString == nil) {
        return NULL;
    }

    const char *cstr = [urlString UTF8String];
    if (cstr == NULL) {
        return NULL;
    }

    size_t len = strlen(cstr);
    char *result = (char *)malloc(len + 1);
    if (result != NULL) {
        memcpy(result, cstr, len + 1);
    }

    return result;
}

// Get current page title (caller must free the result)
char *webview_get_title(void *webview_ptr) {
    if (webview_ptr == NULL) {
        return NULL;
    }

    rust_webview_t *wrapper = (rust_webview_t *)webview_ptr;
    if (wrapper->webview == nil) {
        return NULL;
    }

    NSString *title = [wrapper->webview title];
    if (title == nil || [title length] == 0) {
        return NULL;
    }

    const char *cstr = [title UTF8String];
    if (cstr == NULL) {
        return NULL;
    }

    size_t len = strlen(cstr);
    char *result = (char *)malloc(len + 1);
    if (result != NULL) {
        memcpy(result, cstr, len + 1);
    }

    return result;
}

// Open WebKit Inspector/DevTools
void webview_open_dev_tools(void *webview_ptr) {
    if (webview_ptr == NULL) {
        return;
    }

    rust_webview_t *wrapper = (rust_webview_t *)webview_ptr;
    if (wrapper->webview == nil) {
        return;
    }

    // Trigger WebKit Inspector via private API
    // In development builds, this opens the inspector
    @try {
        [wrapper->webview evaluateJavaScript:@"if (window.__INSPECTOR__) window.__INSPECTOR__.show();" completionHandler:nil];
    } @catch (NSException *exception) {
        // Ignore errors - devtools may not be available
    }
}

// Make the webview lose focus (so keyboard input doesn't go to it)
void webview_lose_focus(void *webview_ptr) {
    if (webview_ptr == NULL) {
        return;
    }

    rust_webview_t *wrapper = (rust_webview_t *)webview_ptr;
    if (wrapper->webview == nil) {
        return;
    }

    // Make the window the key window to take focus away from webview
    NSWindow *window = [wrapper->webview window];
    if (window != nil) {
        [window makeFirstResponder:nil];
    }
}
