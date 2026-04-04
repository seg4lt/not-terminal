#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>
#include <pthread.h>
#include <dispatch/dispatch.h>

#import <AppKit/AppKit.h>
#import <Carbon/Carbon.h>
#import <objc/runtime.h>

#include "../vendor/ghostty/include/ghostty.h"

#define RUST_GHOSTTY_ACTION_QUEUE_CAPACITY 256

typedef enum rust_ghostty_action_tag_e {
  RUST_GHOSTTY_ACTION_NONE = 0,
  RUST_GHOSTTY_ACTION_NEW_SPLIT = 1,
  RUST_GHOSTTY_ACTION_GOTO_SPLIT = 2,
  RUST_GHOSTTY_ACTION_RESIZE_SPLIT = 3,
  RUST_GHOSTTY_ACTION_EQUALIZE_SPLITS = 4,
  RUST_GHOSTTY_ACTION_TOGGLE_SPLIT_ZOOM = 5,
  RUST_GHOSTTY_ACTION_NEW_TAB = 6,
  RUST_GHOSTTY_ACTION_GOTO_TAB = 7,
  RUST_GHOSTTY_ACTION_COMMAND_FINISHED = 8,
  RUST_GHOSTTY_ACTION_RING_BELL = 9,
  RUST_GHOSTTY_ACTION_SET_TITLE = 10,
  RUST_GHOSTTY_ACTION_DESKTOP_NOTIFICATION = 11,
  RUST_GHOSTTY_ACTION_PROGRESS_REPORT = 12,
  RUST_GHOSTTY_ACTION_START_SEARCH = 13,
  RUST_GHOSTTY_ACTION_END_SEARCH = 14,
  RUST_GHOSTTY_ACTION_SEARCH_TOTAL = 15,
  RUST_GHOSTTY_ACTION_SEARCH_SELECTED = 16,
} rust_ghostty_action_tag_t;

typedef struct rust_ghostty_action_event_s {
  uint32_t tag;
  uintptr_t surface;
  intptr_t arg0;
  intptr_t arg1;
  uint16_t amount;
  uint16_t reserved;
  uintptr_t ptr;  // For passing pointers (e.g., copied string payloads)
  char text_copy[256];  // Buffer to copy string payloads immediately
} rust_ghostty_action_event_t;

typedef struct rust_ghostty_runtime_state_s {
  atomic_bool pending_tick;
  pthread_mutex_t action_lock;
  rust_ghostty_action_event_t action_queue[RUST_GHOSTTY_ACTION_QUEUE_CAPACITY];
  uint32_t action_head;
  uint32_t action_tail;
} rust_ghostty_runtime_state_t;

typedef struct rust_ghostty_runtime_bundle_s {
  rust_ghostty_runtime_state_t *state;
  ghostty_runtime_config_s config;
  void *surface;  // Store surface pointer for clipboard callbacks
  void *host_view;  // Store host view for native overlays
} rust_ghostty_runtime_bundle_t;

static atomic_bool rust_ghostty_attention_badge_clicked = false;
static const void *RUST_GHOSTTY_SEARCH_OVERLAY_KEY = &RUST_GHOSTTY_SEARCH_OVERLAY_KEY;

@interface RustGhosttyAttentionBadgeTarget : NSObject
- (void)handleAttentionBadgePress:(id)sender;
@end

static RustGhosttyAttentionBadgeTarget *rust_ghostty_attention_badge_target(void);

@class RustGhosttySearchOverlayController;
static RustGhosttySearchOverlayController *rust_ghostty_search_overlay_controller(NSView *host,
                                                                                  bool create_if_missing);
static NSView *rust_ghostty_find_focusable_terminal_view(NSView *view);

@interface RustGhosttySearchField : NSSearchField
@property(nonatomic, assign) RustGhosttySearchOverlayController *overlayController;
@end

@interface RustGhosttySearchOverlayController : NSObject <NSSearchFieldDelegate> {
 @private
  NSView *_host;
  NSView *_container;
  RustGhosttySearchField *_field;
  NSTextField *_countLabel;
  NSButton *_previousButton;
  NSButton *_nextButton;
  NSButton *_closeButton;
  NSTimer *_debounceTimer;
  void *_surface;
  NSInteger _searchTotal;
  BOOL _hasSearchTotal;
  NSInteger _searchSelected;
  BOOL _hasSearchSelected;
}
- (instancetype)initWithHost:(NSView *)host;
- (void)setSurface:(void *)surface;
- (void)showWithNeedle:(NSString *)needle;
- (void)hide;
- (void)focusField;
- (void)syncFrame;
- (void)setSearchTotal:(NSInteger)total hasTotal:(BOOL)hasTotal;
- (void)setSearchSelected:(NSInteger)selected hasSelected:(BOOL)hasSelected;
- (void)restoreTerminalFocus;
- (void)deactivate;
- (void)navigateSearchPrevious:(BOOL)previous;
- (void)searchSelection;
@end

@implementation RustGhosttyAttentionBadgeTarget
- (void)handleAttentionBadgePress:(id)sender {
  (void)sender;
  atomic_store_explicit(&rust_ghostty_attention_badge_clicked, true, memory_order_release);
}
@end

@implementation RustGhosttySearchField

- (void)keyDown:(NSEvent *)event {
  RustGhosttySearchOverlayController *controller = self.overlayController;
  if (controller != nil) {
    NSString *characters = [event charactersIgnoringModifiers];
    if ([characters length] > 0) {
      unichar ch = [characters characterAtIndex:0];
      if (ch == 0x1B) {
        [controller deactivate];
        return;
      }
    }
    if ([event keyCode] == 53) {
      [controller deactivate];
      return;
    }
  }

  [super keyDown:event];
}

- (BOOL)performKeyEquivalent:(NSEvent *)event {
  RustGhosttySearchOverlayController *controller = self.overlayController;
  if (controller == nil) {
    return [super performKeyEquivalent:event];
  }

  NSEventModifierFlags modifiers = [event modifierFlags] & NSEventModifierFlagDeviceIndependentFlagsMask;
  BOOL command = (modifiers & NSEventModifierFlagCommand) != 0;
  BOOL shift = (modifiers & NSEventModifierFlagShift) != 0;
  BOOL control = (modifiers & NSEventModifierFlagControl) != 0;
  BOOL option = (modifiers & NSEventModifierFlagOption) != 0;
  NSString *characters = [[event charactersIgnoringModifiers] lowercaseString];

  if (command && !control && !option) {
    if ([characters isEqualToString:@"g"]) {
      [controller navigateSearchPrevious:shift];
      return YES;
    }
    if (!shift && [characters isEqualToString:@"e"]) {
      [controller searchSelection];
      return YES;
    }
    if (!shift && [characters isEqualToString:@"f"]) {
      [controller focusField];
      return YES;
    }
  }

  return [super performKeyEquivalent:event];
}

@end

@interface RustGhosttyAttentionBadgeView : NSView {
 @private
  NSImageView *_iconView;
  NSTextField *_countLabel;
}
- (void)setAttentionCount:(int32_t)count;
- (NSSize)preferredBadgeSize;
@end

@implementation RustGhosttyAttentionBadgeView

- (instancetype)initWithFrame:(NSRect)frame {
  self = [super initWithFrame:frame];
  if (self == nil) {
    return nil;
  }

  [self setAutoresizingMask:NSViewNotSizable];
  [self setWantsLayer:YES];
  [[self layer] setCornerRadius:11.0];
  [[self layer] setBorderWidth:1.0];
  [[self layer] setMasksToBounds:YES];
  [[self layer] setBackgroundColor:[[NSColor colorWithCalibratedRed:0.28 green:0.21 blue:0.08 alpha:0.9] CGColor]];
  [[self layer] setBorderColor:[[NSColor colorWithCalibratedRed:0.72 green:0.56 blue:0.2 alpha:0.95] CGColor]];

  _iconView = [[NSImageView alloc] initWithFrame:NSZeroRect];
  [_iconView setImageScaling:NSImageScaleProportionallyDown];
  [_iconView setAutoresizingMask:NSViewNotSizable];
  if ([NSImage respondsToSelector:@selector(imageWithSystemSymbolName:accessibilityDescription:)]) {
    NSImage *icon = [NSImage imageWithSystemSymbolName:@"bell.fill" accessibilityDescription:@"Notifications"];
    if (icon != nil) {
      NSImageSymbolConfiguration *config =
          [NSImageSymbolConfiguration configurationWithPointSize:13.0
                                                         weight:NSFontWeightSemibold
                                                          scale:NSImageSymbolScaleSmall];
      icon = [icon imageWithSymbolConfiguration:config];
      [_iconView setImage:icon];
      [_iconView setContentTintColor:[NSColor colorWithCalibratedRed:0.94 green:0.76 blue:0.24 alpha:0.98]];
    }
  }
  [self addSubview:_iconView];

  _countLabel = [[NSTextField labelWithString:@"0"] retain];
  [_countLabel setFont:[NSFont systemFontOfSize:13.0 weight:NSFontWeightSemibold]];
  [_countLabel setTextColor:[NSColor colorWithCalibratedRed:0.98 green:0.95 blue:0.88 alpha:0.98]];
  [_countLabel setAlignment:NSTextAlignmentCenter];
  [_countLabel setAutoresizingMask:NSViewNotSizable];
  [self addSubview:_countLabel];

  NSClickGestureRecognizer *click =
      [[NSClickGestureRecognizer alloc] initWithTarget:rust_ghostty_attention_badge_target()
                                                action:@selector(handleAttentionBadgePress:)];
  if (click != nil) {
    [self addGestureRecognizer:click];
    [click release];
  }

  return self;
}

- (void)dealloc {
  [_iconView release];
  [_countLabel release];
  [super dealloc];
}

- (void)setAttentionCount:(int32_t)count {
  [_countLabel setStringValue:[NSString stringWithFormat:@"%d", count]];
  [self setNeedsLayout:YES];
}

- (NSSize)preferredBadgeSize {
  [_countLabel sizeToFit];
  NSSize labelSize = [_countLabel fittingSize];
  return NSMakeSize(MAX(62.0, ceil(labelSize.width) + 42.0), 32.0);
}

- (void)layout {
  [super layout];

  const CGFloat iconSize = 14.0;
  const CGFloat gap = 8.0;
  NSRect bounds = [self bounds];
  [_countLabel sizeToFit];
  NSSize labelSize = [_countLabel fittingSize];
  CGFloat totalWidth = iconSize + gap + labelSize.width;
  CGFloat startX = floor((NSWidth(bounds) - totalWidth) * 0.5);
  CGFloat iconY = floor((NSHeight(bounds) - iconSize) * 0.5);
  CGFloat labelY = floor((NSHeight(bounds) - labelSize.height) * 0.5);

  [_iconView setFrame:NSMakeRect(startX, iconY, iconSize, iconSize)];
  [_countLabel setFrame:NSMakeRect(startX + iconSize + gap,
                                   labelY,
                                   ceil(labelSize.width),
                                   ceil(labelSize.height))];
}

@end

@implementation RustGhosttySearchOverlayController

- (instancetype)initWithHost:(NSView *)host {
  self = [super init];
  if (self == nil) {
    return nil;
  }

  _host = host;
  _surface = NULL;
  _debounceTimer = nil;
  _searchTotal = 0;
  _hasSearchTotal = NO;
  _searchSelected = 0;
  _hasSearchSelected = NO;

  _container = [[NSView alloc] initWithFrame:NSZeroRect];
  [_container setHidden:YES];
  [_container setWantsLayer:YES];
  [[_container layer] setCornerRadius:8.0];
  [[_container layer] setBorderWidth:1.0];
  [[_container layer] setMasksToBounds:YES];
  [[_container layer] setBackgroundColor:[[NSColor colorWithCalibratedRed:0.09 green:0.11 blue:0.16 alpha:0.96] CGColor]];
  [[_container layer] setBorderColor:[[NSColor colorWithCalibratedRed:0.18 green:0.20 blue:0.26 alpha:1.0] CGColor]];

  _field = [[RustGhosttySearchField alloc] initWithFrame:NSZeroRect];
  [_field setOverlayController:self];
  [_field setDelegate:self];
  [_field setPlaceholderString:@"Search"];
  [_field setFocusRingType:NSFocusRingTypeNone];
  [_field setBezeled:YES];
  [_field setBordered:YES];
  [_field setFont:[NSFont systemFontOfSize:13.0]];
  [_container addSubview:_field];

  _countLabel = [[NSTextField labelWithString:@""] retain];
  [_countLabel setAlignment:NSTextAlignmentRight];
  [_countLabel setFont:[NSFont monospacedDigitSystemFontOfSize:12.0 weight:NSFontWeightRegular]];
  [_countLabel setTextColor:[NSColor colorWithCalibratedRed:0.70 green:0.74 blue:0.80 alpha:1.0]];
  [_container addSubview:_countLabel];

  _previousButton = [[NSButton buttonWithTitle:@"Prev" target:self action:@selector(previousPressed:)] retain];
  [_previousButton setBezelStyle:NSBezelStyleRounded];
  [_previousButton setFont:[NSFont systemFontOfSize:12.0]];
  [_container addSubview:_previousButton];

  _nextButton = [[NSButton buttonWithTitle:@"Next" target:self action:@selector(nextPressed:)] retain];
  [_nextButton setBezelStyle:NSBezelStyleRounded];
  [_nextButton setFont:[NSFont systemFontOfSize:12.0]];
  [_container addSubview:_nextButton];

  _closeButton = [[NSButton buttonWithTitle:@"Close" target:self action:@selector(closePressed:)] retain];
  [_closeButton setBezelStyle:NSBezelStyleRounded];
  [_closeButton setFont:[NSFont systemFontOfSize:12.0]];
  [_container addSubview:_closeButton];

  [self syncFrame];
  return self;
}

- (void)dealloc {
  [_debounceTimer invalidate];
  _debounceTimer = nil;
  [_container removeFromSuperview];
  [_container release];
  [_field release];
  [_countLabel release];
  [_previousButton release];
  [_nextButton release];
  [_closeButton release];
  [super dealloc];
}

- (void)setSurface:(void *)surface {
  _surface = surface;
}

- (void)sendActionString:(NSString *)action {
  if (_surface == NULL || action == nil) {
    return;
  }

  const char *utf8 = [action UTF8String];
  if (utf8 == NULL) {
    return;
  }

  ghostty_surface_binding_action((ghostty_surface_t)_surface, utf8, (uintptr_t)strlen(utf8));
}

- (void)dispatchSearchNow {
  [_debounceTimer invalidate];
  _debounceTimer = nil;
  NSString *action = [NSString stringWithFormat:@"search:%@", [_field stringValue]];
  [self sendActionString:action];
}

- (void)debouncedSearchTimerFired:(NSTimer *)timer {
  if ([timer userInfo] != self) {
    return;
  }
  [self dispatchSearchNow];
}

- (void)scheduleSearchForCurrentText {
  [_debounceTimer invalidate];
  _debounceTimer = nil;

  NSString *value = [_field stringValue];
  if (value == nil) {
    value = @"";
  }

  if ([value length] == 0 || [value length] >= 3) {
    [self dispatchSearchNow];
    return;
  }

  _debounceTimer = [[NSTimer scheduledTimerWithTimeInterval:0.3
                                                     target:self
                                                   selector:@selector(debouncedSearchTimerFired:)
                                                   userInfo:self
                                                    repeats:NO] retain];
}

- (void)showWithNeedle:(NSString *)needle {
  if (needle == nil) {
    needle = @"";
  }

  [_field setStringValue:needle];
  _hasSearchSelected = NO;
  _hasSearchTotal = NO;
  [self refreshCountLabel];
  [self syncFrame];

  NSView *parent = [_host superview];
  if (parent != nil && [_container superview] != parent) {
    [parent addSubview:_container positioned:NSWindowAbove relativeTo:_host];
  } else if (parent != nil) {
    [parent addSubview:_container positioned:NSWindowAbove relativeTo:_host];
  }

  [_container setHidden:NO];
  [self focusField];

  if ([needle length] > 0) {
    [self scheduleSearchForCurrentText];
  } else {
    [_debounceTimer invalidate];
    _debounceTimer = nil;
  }
}

- (void)hide {
  [_debounceTimer invalidate];
  _debounceTimer = nil;
  [_container setHidden:YES];
  if ([_container superview] != nil) {
    [_container removeFromSuperview];
  }
}

- (void)focusField {
  if ([_container isHidden]) {
    return;
  }

  NSWindow *window = [_host window];
  if (window != nil) {
    [window makeFirstResponder:_field];
  }
}

- (void)syncFrame {
  NSView *parent = [_host superview];
  if (parent == nil) {
    return;
  }

  const CGFloat width = 420.0;
  const CGFloat height = 38.0;
  const CGFloat inset = 12.0;
  NSRect hostFrame = [_host frame];
  CGFloat x = NSMinX(hostFrame) + NSWidth(hostFrame) - width - inset;
  CGFloat y = [parent isFlipped]
                  ? (NSMinY(hostFrame) + inset)
                  : (NSMaxY(hostFrame) - height - inset);

  [_container setFrame:NSMakeRect(x, y, width, height)];
  [_field setFrame:NSMakeRect(10.0, 8.0, 200.0, 22.0)];
  [_countLabel setFrame:NSMakeRect(220.0, 10.0, 54.0, 18.0)];
  [_previousButton setFrame:NSMakeRect(282.0, 7.0, 42.0, 24.0)];
  [_nextButton setFrame:NSMakeRect(328.0, 7.0, 42.0, 24.0)];
  [_closeButton setFrame:NSMakeRect(374.0, 7.0, 38.0, 24.0)];
}

- (void)refreshCountLabel {
  NSString *label = @"";
  if (_hasSearchSelected) {
    NSString *totalLabel = _hasSearchTotal ? [NSString stringWithFormat:@"%ld", (long)_searchTotal] : @"?";
    label = [NSString stringWithFormat:@"%ld/%@", (long)(_searchSelected + 1), totalLabel];
  } else if (_hasSearchTotal) {
    label = [NSString stringWithFormat:@"-/%ld", (long)_searchTotal];
  }
  [_countLabel setStringValue:label];
}

- (void)setSearchTotal:(NSInteger)total hasTotal:(BOOL)hasTotal {
  _searchTotal = total;
  _hasSearchTotal = hasTotal;
  [self refreshCountLabel];
}

- (void)setSearchSelected:(NSInteger)selected hasSelected:(BOOL)hasSelected {
  _searchSelected = selected;
  _hasSearchSelected = hasSelected;
  [self refreshCountLabel];
}

- (void)restoreTerminalFocus {
  NSWindow *window = [_host window];
  if (window == nil) {
    return;
  }

  NSView *candidate = rust_ghostty_find_focusable_terminal_view(_host);
  if (candidate == nil) {
    candidate = _host;
  }

  if (candidate != nil) {
    [window makeFirstResponder:candidate];
  }
}

- (void)deactivate {
  if ([_container isHidden]) {
    return;
  }
  [self sendActionString:@"end_search"];
  [self hide];
  [self restoreTerminalFocus];
}

- (void)navigateSearchPrevious:(BOOL)previous {
  [self sendActionString:(previous ? @"navigate_search:previous" : @"navigate_search:next")];
  [self focusField];
}

- (void)searchSelection {
  [self sendActionString:@"search_selection"];
}

- (void)previousPressed:(id)sender {
  (void)sender;
  [self navigateSearchPrevious:YES];
}

- (void)nextPressed:(id)sender {
  (void)sender;
  [self navigateSearchPrevious:NO];
}

- (void)closePressed:(id)sender {
  (void)sender;
  [self deactivate];
}

- (void)controlTextDidChange:(NSNotification *)notification {
  if ([notification object] != _field) {
    return;
  }
  _hasSearchSelected = NO;
  _hasSearchTotal = NO;
  [self refreshCountLabel];
  [self scheduleSearchForCurrentText];
}

- (BOOL)control:(NSControl *)control textView:(NSTextView *)textView doCommandBySelector:(SEL)commandSelector {
  (void)control;
  (void)textView;

  if (commandSelector == @selector(insertNewline:)) {
    NSEventModifierFlags modifiers = [[NSApp currentEvent] modifierFlags] & NSEventModifierFlagDeviceIndependentFlagsMask;
    BOOL previous = (modifiers & NSEventModifierFlagShift) != 0;
    [self navigateSearchPrevious:previous];
    return YES;
  }

  if (commandSelector == @selector(cancelOperation:)) {
    [self deactivate];
    return YES;
  }

  return NO;
}

@end

static RustGhosttyAttentionBadgeTarget *rust_ghostty_attention_badge_target(void) {
  static RustGhosttyAttentionBadgeTarget *target = nil;
  if (target == nil) {
    target = [[RustGhosttyAttentionBadgeTarget alloc] init];
  }
  return target;
}

static NSView *rust_ghostty_find_focusable_terminal_view(NSView *view) {
  if (view == nil) {
    return nil;
  }

  NSArray<NSView *> *subviews = [view subviews];
  for (NSView *subview in [subviews reverseObjectEnumerator]) {
    NSView *candidate = rust_ghostty_find_focusable_terminal_view(subview);
    if (candidate != nil) {
      return candidate;
    }
  }

  if ([view acceptsFirstResponder]) {
    return view;
  }

  return nil;
}

static void rust_ghostty_wakeup_cb(void *userdata) {
  rust_ghostty_runtime_state_t *state = (rust_ghostty_runtime_state_t *)userdata;
  if (state == NULL) {
    return;
  }

  atomic_store_explicit(&state->pending_tick, true, memory_order_release);
}

static void rust_ghostty_enqueue_action(
    rust_ghostty_runtime_state_t *state,
    rust_ghostty_action_event_t action_event) {
  if (state == NULL) {
    return;
  }

  if (pthread_mutex_lock(&state->action_lock) != 0) {
    return;
  }

  const uint32_t head = state->action_head;
  const uint32_t tail = state->action_tail;
  const uint32_t next_tail = (tail + 1) % RUST_GHOSTTY_ACTION_QUEUE_CAPACITY;

  if (next_tail == head) {
    state->action_head = (head + 1) % RUST_GHOSTTY_ACTION_QUEUE_CAPACITY;
  }

  state->action_queue[tail] = action_event;
  state->action_tail = next_tail;

  pthread_mutex_unlock(&state->action_lock);
  atomic_store_explicit(&state->pending_tick, true, memory_order_release);
}

static bool rust_ghostty_action_cb(ghostty_app_t app,
                                   ghostty_target_s target,
                                   ghostty_action_s action) {
  rust_ghostty_runtime_state_t *state =
      (rust_ghostty_runtime_state_t *)ghostty_app_userdata(app);
  if (state == NULL) {
    return false;
  }

  uintptr_t surface_ptr = 0;
  rust_ghostty_runtime_bundle_t *surface_bundle = NULL;
  NSView *surface_host = nil;
  if (target.tag == GHOSTTY_TARGET_SURFACE) {
    surface_ptr = (uintptr_t)target.target.surface;
    surface_bundle =
        (rust_ghostty_runtime_bundle_t *)ghostty_surface_userdata(target.target.surface);
    if (surface_bundle != NULL) {
      surface_host = (NSView *)surface_bundle->host_view;
    }
  }

  rust_ghostty_action_event_t action_event = {
      .tag = RUST_GHOSTTY_ACTION_NONE,
      .surface = surface_ptr,
      .arg0 = 0,
      .arg1 = 0,
      .amount = 0,
      .reserved = 0,
  };

  switch (action.tag) {
    case GHOSTTY_ACTION_NEW_SPLIT:
      action_event.tag = RUST_GHOSTTY_ACTION_NEW_SPLIT;
      action_event.arg0 = (int32_t)action.action.new_split;
      break;
    case GHOSTTY_ACTION_GOTO_SPLIT:
      action_event.tag = RUST_GHOSTTY_ACTION_GOTO_SPLIT;
      action_event.arg0 = (int32_t)action.action.goto_split;
      break;
    case GHOSTTY_ACTION_RESIZE_SPLIT:
      action_event.tag = RUST_GHOSTTY_ACTION_RESIZE_SPLIT;
      action_event.arg0 = (int32_t)action.action.resize_split.direction;
      action_event.amount = (uint16_t)action.action.resize_split.amount;
      break;
    case GHOSTTY_ACTION_EQUALIZE_SPLITS:
      action_event.tag = RUST_GHOSTTY_ACTION_EQUALIZE_SPLITS;
      break;
    case GHOSTTY_ACTION_TOGGLE_SPLIT_ZOOM:
      action_event.tag = RUST_GHOSTTY_ACTION_TOGGLE_SPLIT_ZOOM;
      break;
    case GHOSTTY_ACTION_NEW_TAB:
      action_event.tag = RUST_GHOSTTY_ACTION_NEW_TAB;
      break;
    case GHOSTTY_ACTION_GOTO_TAB:
      action_event.tag = RUST_GHOSTTY_ACTION_GOTO_TAB;
      action_event.arg0 = (int32_t)action.action.goto_tab;
      break;
    case GHOSTTY_ACTION_COMMAND_FINISHED:
      action_event.tag = RUST_GHOSTTY_ACTION_COMMAND_FINISHED;
      action_event.arg0 = (int32_t)action.action.command_finished.exit_code;
      break;
    case GHOSTTY_ACTION_RING_BELL:
      action_event.tag = RUST_GHOSTTY_ACTION_RING_BELL;
      break;
    case GHOSTTY_ACTION_SET_TITLE: {
      action_event.tag = RUST_GHOSTTY_ACTION_SET_TITLE;
      // Copy the title string immediately - the original pointer may be freed
      const char *title = action.action.set_title.title;
      if (title != NULL) {
        strncpy(action_event.text_copy, title, sizeof(action_event.text_copy) - 1);
        action_event.text_copy[sizeof(action_event.text_copy) - 1] = '\0';
        action_event.ptr = (uintptr_t)action_event.text_copy;
      } else {
        action_event.text_copy[0] = '\0';
        action_event.ptr = 0;
      }
      break;
    }
    case GHOSTTY_ACTION_DESKTOP_NOTIFICATION: {
      action_event.tag = RUST_GHOSTTY_ACTION_DESKTOP_NOTIFICATION;
      break;
    }
    case GHOSTTY_ACTION_PROGRESS_REPORT: {
      action_event.tag = RUST_GHOSTTY_ACTION_PROGRESS_REPORT;
      action_event.arg0 = (int32_t)action.action.progress_report.state;
      action_event.arg1 = (int32_t)action.action.progress_report.progress;
      break;
    }
    case GHOSTTY_ACTION_START_SEARCH: {
      action_event.tag = RUST_GHOSTTY_ACTION_START_SEARCH;
      const char *needle = action.action.start_search.needle;
      if (needle != NULL) {
        strncpy(action_event.text_copy, needle, sizeof(action_event.text_copy) - 1);
        action_event.text_copy[sizeof(action_event.text_copy) - 1] = '\0';
        action_event.ptr = (uintptr_t)action_event.text_copy;
      } else {
        action_event.text_copy[0] = '\0';
        action_event.ptr = 0;
      }
      if (surface_host != nil) {
        char *needle_copy = (needle != NULL) ? strdup(needle) : NULL;
        dispatch_async(dispatch_get_main_queue(), ^{
          RustGhosttySearchOverlayController *controller =
              rust_ghostty_search_overlay_controller(surface_host, true);
          if (surface_bundle != NULL) {
            [controller setSurface:surface_bundle->surface];
          }
          NSString *text = (needle_copy != NULL)
                               ? [NSString stringWithUTF8String:needle_copy]
                               : @"";
          [controller showWithNeedle:(text != nil ? text : @"")];
          if (needle_copy != NULL) {
            free(needle_copy);
          }
        });
      }
      break;
    }
    case GHOSTTY_ACTION_END_SEARCH:
      action_event.tag = RUST_GHOSTTY_ACTION_END_SEARCH;
      if (surface_host != nil) {
        dispatch_async(dispatch_get_main_queue(), ^{
          RustGhosttySearchOverlayController *controller =
              rust_ghostty_search_overlay_controller(surface_host, false);
          [controller hide];
        });
      }
      break;
    case GHOSTTY_ACTION_SEARCH_TOTAL:
      action_event.tag = RUST_GHOSTTY_ACTION_SEARCH_TOTAL;
      action_event.arg0 = (intptr_t)action.action.search_total.total;
      if (surface_host != nil) {
        const ssize_t total = action.action.search_total.total;
        dispatch_async(dispatch_get_main_queue(), ^{
          RustGhosttySearchOverlayController *controller =
              rust_ghostty_search_overlay_controller(surface_host, false);
          [controller setSearchTotal:(NSInteger)total hasTotal:(total >= 0)];
        });
      }
      break;
    case GHOSTTY_ACTION_SEARCH_SELECTED:
      action_event.tag = RUST_GHOSTTY_ACTION_SEARCH_SELECTED;
      action_event.arg0 = (intptr_t)action.action.search_selected.selected;
      if (surface_host != nil) {
        const ssize_t selected = action.action.search_selected.selected;
        dispatch_async(dispatch_get_main_queue(), ^{
          RustGhosttySearchOverlayController *controller =
              rust_ghostty_search_overlay_controller(surface_host, false);
          [controller setSearchSelected:(NSInteger)selected hasSelected:(selected >= 0)];
        });
      }
      break;
    default:
      return false;
  }

  rust_ghostty_enqueue_action(state, action_event);
  return true;
}

static bool rust_ghostty_read_clipboard_cb(void *userdata,
                                           ghostty_clipboard_e location,
                                           void *state) {
  (void)location;
  // userdata is the runtime bundle (set via surface config)
  rust_ghostty_runtime_bundle_t *bundle =
      (rust_ghostty_runtime_bundle_t *)userdata;
  if (bundle == NULL) {
    return false;
  }

  ghostty_surface_t surface = (ghostty_surface_t)bundle->surface;
  if (surface == NULL) {
    return false;
  }

  NSPasteboard *pasteboard = [NSPasteboard generalPasteboard];
  NSString *content = [pasteboard stringForType:NSPasteboardTypeString];
  if (content == nil) {
    content = @"";
  }

  // Complete the clipboard request with the data
  ghostty_surface_complete_clipboard_request(surface, [content UTF8String], state, false);
  return true;
}

static void rust_ghostty_confirm_read_clipboard_cb(
    void *userdata,
    const char *value,
    void *state,
    ghostty_clipboard_request_e request) {
  (void)request;
  // Ghostty embedded API requires the host to complete the pending clipboard
  // request after this callback (vendor/ghostty/src/apprt/embedded.zig:
  // confirm_read_clipboard + ghostty_surface_complete_clipboard_request).
  rust_ghostty_runtime_bundle_t *bundle =
      (rust_ghostty_runtime_bundle_t *)userdata;
  if (bundle == NULL || state == NULL) {
    return;
  }

  ghostty_surface_t surface = (ghostty_surface_t)bundle->surface;
  if (surface == NULL) {
    return;
  }

  const char *content = (value != NULL) ? value : "";
  ghostty_surface_complete_clipboard_request(surface, content, state, true);
}

static void rust_ghostty_write_clipboard_cb(
    void *userdata,
    ghostty_clipboard_e location,
    const ghostty_clipboard_content_s *content,
    size_t len,
    bool requires_confirmation) {
  (void)userdata;
  (void)location;
  (void)requires_confirmation;
  // userdata might contain info about which surface, but for write we
  // don't need the surface pointer since we're just writing to pasteboard
  if (content == NULL || len == 0) {
    return;
  }

  NSPasteboard *pasteboard = [NSPasteboard generalPasteboard];

  // Find text/plain content
  for (size_t i = 0; i < len; i++) {
    if (content[i].mime == NULL || content[i].data == NULL) {
      continue;
    }

    NSString *mime = [NSString stringWithUTF8String:content[i].mime];
    if ([mime isEqualToString:@"text/plain"]) {
      NSString *text = [NSString stringWithUTF8String:content[i].data];
      [pasteboard clearContents];
      [pasteboard setString:text forType:NSPasteboardTypeString];
      break;
    }
  }
}

static void rust_ghostty_close_surface_cb(void *userdata, bool process_alive) {
  (void)userdata;
  (void)process_alive;
}

rust_ghostty_runtime_bundle_t *rust_ghostty_runtime_bundle_new(void) {
  rust_ghostty_runtime_state_t *state =
      (rust_ghostty_runtime_state_t *)malloc(sizeof(rust_ghostty_runtime_state_t));
  if (state == NULL) {
    return NULL;
  }

  atomic_init(&state->pending_tick, false);
  state->action_head = 0;
  state->action_tail = 0;
  if (pthread_mutex_init(&state->action_lock, NULL) != 0) {
    free(state);
    return NULL;
  }

  rust_ghostty_runtime_bundle_t *bundle =
      (rust_ghostty_runtime_bundle_t *)malloc(sizeof(rust_ghostty_runtime_bundle_t));
  if (bundle == NULL) {
    free(state);
    return NULL;
  }

  bundle->state = state;
  bundle->surface = NULL;
  bundle->host_view = NULL;
  bundle->config.userdata = state;  // Used by action callbacks via ghostty_app_userdata
  bundle->config.supports_selection_clipboard = false;
  bundle->config.wakeup_cb = rust_ghostty_wakeup_cb;
  bundle->config.action_cb = rust_ghostty_action_cb;
  bundle->config.read_clipboard_cb = rust_ghostty_read_clipboard_cb;
  bundle->config.confirm_read_clipboard_cb = rust_ghostty_confirm_read_clipboard_cb;
  bundle->config.write_clipboard_cb = rust_ghostty_write_clipboard_cb;
  bundle->config.close_surface_cb = rust_ghostty_close_surface_cb;

  return bundle;
}

void rust_ghostty_runtime_bundle_free(rust_ghostty_runtime_bundle_t *bundle) {
  if (bundle == NULL) {
    return;
  }

  pthread_mutex_destroy(&bundle->state->action_lock);
  free(bundle->state);
  bundle->state = NULL;
  free(bundle);
}

const void *rust_ghostty_runtime_config_ptr(
    const rust_ghostty_runtime_bundle_t *bundle) {
  if (bundle == NULL) {
    return NULL;
  }

  return &bundle->config;
}

bool rust_ghostty_runtime_take_pending_tick(
    const rust_ghostty_runtime_bundle_t *bundle) {
  if (bundle == NULL || bundle->state == NULL) {
    return false;
  }

  return atomic_exchange_explicit(
      &bundle->state->pending_tick,
      false,
      memory_order_acq_rel);
}

bool rust_ghostty_runtime_has_pending_tick(
    const rust_ghostty_runtime_bundle_t *bundle) {
  if (bundle == NULL || bundle->state == NULL) {
    return false;
  }

  return atomic_load_explicit(&bundle->state->pending_tick, memory_order_acquire);
}

bool rust_ghostty_runtime_take_pending_action(
    const rust_ghostty_runtime_bundle_t *bundle,
    rust_ghostty_action_event_t *out_action_event) {
  if (bundle == NULL || bundle->state == NULL || out_action_event == NULL) {
    return false;
  }

  rust_ghostty_runtime_state_t *state = bundle->state;
  if (pthread_mutex_lock(&state->action_lock) != 0) {
    return false;
  }

  const bool has_action = state->action_head != state->action_tail;
  if (!has_action) {
    pthread_mutex_unlock(&state->action_lock);
    return false;
  }

  *out_action_event = state->action_queue[state->action_head];
  state->action_head = (state->action_head + 1) % RUST_GHOSTTY_ACTION_QUEUE_CAPACITY;

  pthread_mutex_unlock(&state->action_lock);
  return true;
}

uint32_t rust_ghostty_runtime_action_queue_len(
    const rust_ghostty_runtime_bundle_t *bundle) {
  if (bundle == NULL || bundle->state == NULL) {
    return 0;
  }

  rust_ghostty_runtime_state_t *state = bundle->state;
  if (pthread_mutex_lock(&state->action_lock) != 0) {
    return 0;
  }

  uint32_t len;
  if (state->action_tail >= state->action_head) {
    len = state->action_tail - state->action_head;
  } else {
    len = (RUST_GHOSTTY_ACTION_QUEUE_CAPACITY - state->action_head) + state->action_tail;
  }

  pthread_mutex_unlock(&state->action_lock);
  return len;
}

typedef void *(*rust_ghostty_surface_new_fn)(void *, const ghostty_surface_config_s *);

void *rust_ghostty_surface_new_macos(void *surface_new_fn_raw,
                                     void *app,
                                     void *ns_view,
                                     double scale_factor,
                                     float font_size_points,
                                     const char *working_directory,
                                     void *runtime_bundle) {
  if (surface_new_fn_raw == NULL || app == NULL || ns_view == NULL) {
    return NULL;
  }

  rust_ghostty_surface_new_fn surface_new_fn =
      (rust_ghostty_surface_new_fn)surface_new_fn_raw;

  // Create a temporary config - userdata will be set after surface creation
  ghostty_surface_config_s config = {0};
  config.platform_tag = GHOSTTY_PLATFORM_MACOS;
  config.platform.macos.nsview = ns_view;
  config.scale_factor = scale_factor;
  config.font_size = font_size_points;
  config.working_directory = working_directory;
  config.context = GHOSTTY_SURFACE_CONTEXT_WINDOW;
  config.userdata = runtime_bundle;  // Pass the bundle so we can find it later

  void *surface = surface_new_fn(app, &config);
  if (surface != NULL && runtime_bundle != NULL) {
    rust_ghostty_runtime_bundle_t *bundle =
        (rust_ghostty_runtime_bundle_t *)runtime_bundle;
    bundle->surface = surface;
    bundle->host_view = ns_view;
    RustGhosttySearchOverlayController *controller =
        rust_ghostty_search_overlay_controller((NSView *)ns_view, true);
    [controller setSurface:surface];
  }

  return surface;
}

void *rust_ghostty_host_view_new(void *parent_ns_view) {
  if (parent_ns_view == NULL) {
    return NULL;
  }

  NSView *parent = (NSView *)parent_ns_view;
  NSRect frame = parent.bounds;
  NSView *host = [[NSView alloc] initWithFrame:frame];
  if (host == nil) {
    return NULL;
  }

  [host setHidden:YES];
  [host setAutoresizingMask:NSViewWidthSizable | NSViewHeightSizable];
  [parent addSubview:host];

  return (void *)host;
}

static const void *RUST_GHOSTTY_SPLIT_BADGE_KEY = &RUST_GHOSTTY_SPLIT_BADGE_KEY;
static const void *RUST_GHOSTTY_ATTENTION_BADGE_KEY = &RUST_GHOSTTY_ATTENTION_BADGE_KEY;

static RustGhosttySearchOverlayController *rust_ghostty_search_overlay_controller(NSView *host,
                                                                                  bool create_if_missing) {
  if (host == nil) {
    return nil;
  }

  RustGhosttySearchOverlayController *existing =
      (RustGhosttySearchOverlayController *)objc_getAssociatedObject(host, RUST_GHOSTTY_SEARCH_OVERLAY_KEY);
  if (existing != nil || !create_if_missing) {
    return existing;
  }

  RustGhosttySearchOverlayController *controller =
      [[RustGhosttySearchOverlayController alloc] initWithHost:host];
  if (controller == nil) {
    return nil;
  }

  objc_setAssociatedObject(host,
                           RUST_GHOSTTY_SEARCH_OVERLAY_KEY,
                           controller,
                           OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  [controller release];
  return controller;
}

static NSTextField *rust_ghostty_split_badge_label(NSView *host, bool create_if_missing) {
  if (host == nil) {
    return nil;
  }

  NSTextField *existing = (NSTextField *)objc_getAssociatedObject(host, RUST_GHOSTTY_SPLIT_BADGE_KEY);
  if (existing != nil || !create_if_missing) {
    return existing;
  }

  NSTextField *label = [NSTextField labelWithString:@"◫"];
  if (label == nil) {
    return nil;
  }

  [label setAlignment:NSTextAlignmentCenter];
  [label setFont:[NSFont systemFontOfSize:11.0 weight:NSFontWeightSemibold]];
  [label setSelectable:NO];
  [label setEditable:NO];
  [label setBezeled:NO];
  [label setBordered:NO];
  [label setDrawsBackground:YES];
  [label setBackgroundColor:[NSColor colorWithCalibratedWhite:0.08 alpha:0.72]];
  [label setTextColor:[NSColor colorWithCalibratedWhite:0.92 alpha:0.95]];
  [label setAutoresizingMask:NSViewNotSizable];
  [label setWantsLayer:YES];
  [[label layer] setCornerRadius:4.0];
  [[label layer] setBorderWidth:1.0];
  [[label layer] setBorderColor:[[NSColor colorWithCalibratedWhite:0.55 alpha:0.55] CGColor]];
  objc_setAssociatedObject(
      host,
      RUST_GHOSTTY_SPLIT_BADGE_KEY,
      label,
      OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  return label;
}

static RustGhosttyAttentionBadgeView *rust_ghostty_attention_badge_label(NSView *parent, bool create_if_missing) {
  if (parent == nil) {
    return nil;
  }

  RustGhosttyAttentionBadgeView *existing =
      (RustGhosttyAttentionBadgeView *)objc_getAssociatedObject(parent, RUST_GHOSTTY_ATTENTION_BADGE_KEY);
  if (existing != nil || !create_if_missing) {
    return existing;
  }

  RustGhosttyAttentionBadgeView *label = [[RustGhosttyAttentionBadgeView alloc] initWithFrame:NSZeroRect];
  if (label == nil) {
    return nil;
  }

  objc_setAssociatedObject(
      parent,
      RUST_GHOSTTY_ATTENTION_BADGE_KEY,
      label,
      OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  [label release];
  return label;
}

void rust_ghostty_host_view_set_split_badge(void *host_ns_view,
                                            bool visible,
                                            bool active) {
  if (host_ns_view == NULL) {
    return;
  }

  NSView *host = (NSView *)host_ns_view;
  NSView *parent = [host superview];
  if (parent == nil) {
    return;
  }

  NSTextField *label = rust_ghostty_split_badge_label(host, visible);
  if (label == nil) {
    return;
  }

  if (!visible) {
    [label setHidden:YES];
    if ([label superview] != nil) {
      [label removeFromSuperview];
    }
    return;
  }

  const CGFloat width = 18.0;
  const CGFloat height = 14.0;
  const CGFloat inset = 6.0;
  NSRect frame = [host frame];
  CGFloat x = NSMinX(frame) + NSWidth(frame) - width - inset;
  CGFloat y = [parent isFlipped]
                  ? (NSMinY(frame) + inset)
                  : (NSMaxY(frame) - height - inset);

  if ([label superview] != parent) {
    [parent addSubview:label positioned:NSWindowAbove relativeTo:host];
  } else {
    [parent addSubview:label positioned:NSWindowAbove relativeTo:nil];
  }

  [label setFrame:NSMakeRect(x, y, width, height)];
  [label setTextColor:active
                        ? [NSColor colorWithCalibratedRed:0.89 green:0.92 blue:0.98 alpha:0.98]
                        : [NSColor colorWithCalibratedRed:0.75 green:0.80 blue:0.88 alpha:0.92]];
  [label setBackgroundColor:active
                             ? [NSColor colorWithCalibratedWhite:0.07 alpha:0.86]
                             : [NSColor colorWithCalibratedWhite:0.08 alpha:0.72]];
  [[label layer] setBorderColor:[active
                                    ? [NSColor colorWithCalibratedRed:0.68 green:0.74 blue:0.88 alpha:0.95]
                                    : [NSColor colorWithCalibratedWhite:0.55 alpha:0.55]
                                  CGColor]];
  [label setHidden:NO];
}

void rust_ghostty_parent_view_set_attention_badge(void *parent_ns_view,
                                                  bool visible,
                                                  int32_t count) {
  if (parent_ns_view == NULL) {
    return;
  }

  NSView *parent = (NSView *)parent_ns_view;
  RustGhosttyAttentionBadgeView *label = rust_ghostty_attention_badge_label(parent, visible);
  if (label == nil) {
    return;
  }

  if (!visible || count <= 0) {
    [label setHidden:YES];
    if ([label superview] != nil) {
      [label removeFromSuperview];
    }
    return;
  }

  [label setAttentionCount:count];

  NSSize fit = [label preferredBadgeSize];
  const CGFloat width = fit.width;
  const CGFloat height = fit.height;
  const CGFloat inset = 12.0;
  NSRect bounds = [parent bounds];
  CGFloat x = NSMaxX(bounds) - width - inset;
  CGFloat y = [parent isFlipped]
                  ? (NSMinY(bounds) + inset)
                  : (NSMaxY(bounds) - height - inset);

  if ([label superview] != parent) {
    [parent addSubview:label positioned:NSWindowAbove relativeTo:nil];
  } else {
    [parent addSubview:label positioned:NSWindowAbove relativeTo:nil];
  }

  [label setFrame:NSMakeRect(x, y, width, height)];
  [label setHidden:NO];
}

void rust_ghostty_parent_view_reclaim_focus(void *parent_ns_view) {
  if (parent_ns_view == NULL) {
    return;
  }

  NSView *parent = (NSView *)parent_ns_view;
  NSWindow *window = [parent window];
  if (window == nil) {
    return;
  }

  NSResponder *fr = [window firstResponder];
  if (fr == (NSResponder *)parent) {
    return;
  }

  [window makeFirstResponder:parent];
}

bool rust_ghostty_take_pending_attention_badge_click(void) {
  return atomic_exchange_explicit(&rust_ghostty_attention_badge_clicked,
                                  false,
                                  memory_order_acq_rel);
}

void rust_ghostty_host_view_set_frame(void *host_ns_view,
                                      double x,
                                      double y,
                                      double width,
                                      double height) {
  if (host_ns_view == NULL) {
    return;
  }

  NSView *host = (NSView *)host_ns_view;
  NSRect frame = NSMakeRect((CGFloat)x,
                            (CGFloat)y,
                            (CGFloat)(width < 1.0 ? 1.0 : width),
                            (CGFloat)(height < 1.0 ? 1.0 : height));
  [host setFrame:frame];
  RustGhosttySearchOverlayController *controller =
      rust_ghostty_search_overlay_controller(host, false);
  [controller syncFrame];
}

void rust_ghostty_host_view_set_hidden(void *host_ns_view, bool hidden) {
  if (host_ns_view == NULL) {
    return;
  }

  NSView *host = (NSView *)host_ns_view;
  [host setHidden:hidden ? YES : NO];
  RustGhosttySearchOverlayController *controller =
      rust_ghostty_search_overlay_controller(host, false);
  if (hidden) {
    [controller deactivate];

    // macOS does not automatically resign first responder when a view is
    // hidden.  If the terminal surface (or any child) still holds it, keyboard
    // events will be consumed by the hidden view instead of reaching the Iced
    // widget tree (e.g. a modal text input opened with Cmd+P).
    NSWindow *window = [host window];
    if (window != nil) {
      NSResponder *fr = [window firstResponder];
      if ([fr isKindOfClass:[NSView class]]) {
        NSView *frView = (NSView *)fr;
        if (frView == host || [frView isDescendantOf:host]) {
          [window makeFirstResponder:[window contentView]];
        }
      }
    }
  } else {
    [controller syncFrame];
  }
}

void rust_ghostty_host_view_set_search_active(void *host_ns_view, bool active) {
  if (host_ns_view == NULL) {
    return;
  }

  NSView *host = (NSView *)host_ns_view;
  RustGhosttySearchOverlayController *controller =
      rust_ghostty_search_overlay_controller(host, false);
  if (!active) {
    [controller deactivate];
  }
}

void rust_ghostty_host_view_focus_search(void *host_ns_view) {
  if (host_ns_view == NULL) {
    return;
  }

  NSView *host = (NSView *)host_ns_view;
  RustGhosttySearchOverlayController *controller =
      rust_ghostty_search_overlay_controller(host, false);
  [controller focusField];
}

void rust_ghostty_host_view_focus_terminal(void *host_ns_view) {
  if (host_ns_view == NULL) {
    return;
  }

  NSView *host = (NSView *)host_ns_view;
  RustGhosttySearchOverlayController *controller =
      rust_ghostty_search_overlay_controller(host, false);
  [controller restoreTerminalFocus];
}

void rust_ghostty_host_view_free(void *host_ns_view) {
  if (host_ns_view == NULL) {
    return;
  }

  NSView *host = (NSView *)host_ns_view;
  NSTextField *label = (NSTextField *)objc_getAssociatedObject(host, RUST_GHOSTTY_SPLIT_BADGE_KEY);
  if (label != nil) {
    [label removeFromSuperview];
    objc_setAssociatedObject(host, RUST_GHOSTTY_SPLIT_BADGE_KEY, nil, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  }
  RustGhosttySearchOverlayController *search_controller =
      rust_ghostty_search_overlay_controller(host, false);
  if (search_controller != nil) {
    [search_controller hide];
    objc_setAssociatedObject(host, RUST_GHOSTTY_SEARCH_OVERLAY_KEY, nil, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  }
  [host removeFromSuperview];
  [host release];
}

static void rust_ghostty_strip_hide_shortcuts_from_menu(NSMenu *menu) {
  if (menu == nil) {
    return;
  }

  for (NSMenuItem *item in [menu itemArray]) {
    SEL action = [item action];
    if (action == @selector(hide:) || action == @selector(hideOtherApplications:)) {
      [item setKeyEquivalent:@""];
      [item setKeyEquivalentModifierMask:0];
    }

    NSMenu *submenu = [item submenu];
    if (submenu != nil) {
      rust_ghostty_strip_hide_shortcuts_from_menu(submenu);
    }
  }
}

static EventHotKeyRef rust_ghostty_focus_toggle_hotkey_ref = NULL;
static EventHandlerRef rust_ghostty_focus_toggle_handler_ref = NULL;
static NSRunningApplication *rust_ghostty_previous_front_app = nil;

static OSStatus rust_ghostty_focus_toggle_hotkey_handler(
    EventHandlerCallRef _next_handler,
    EventRef event,
    void *_user_data) {
  (void)_next_handler;
  (void)_user_data;

  if (GetEventClass(event) != kEventClassKeyboard ||
      GetEventKind(event) != kEventHotKeyPressed) {
    return noErr;
  }

  if (NSApp == nil) {
    return noErr;
  }

  if ([NSApp isActive]) {
    if (rust_ghostty_previous_front_app != nil &&
        ![rust_ghostty_previous_front_app isTerminated]) {
      [rust_ghostty_previous_front_app activateWithOptions:0];
    }
    return noErr;
  }

  NSRunningApplication *front = [[NSWorkspace sharedWorkspace] frontmostApplication];
  if (front != nil &&
      ![front.bundleIdentifier isEqualToString:[[NSRunningApplication currentApplication] bundleIdentifier]]) {
    if (rust_ghostty_previous_front_app != front) {
      [rust_ghostty_previous_front_app release];
      rust_ghostty_previous_front_app = [front retain];
    }
  }

  [NSApp activateIgnoringOtherApps:YES];
  return noErr;
}

void rust_ghostty_register_focus_toggle_hotkey(void) {
  if (![NSThread isMainThread]) {
    return;
  }

  if (rust_ghostty_focus_toggle_hotkey_ref != NULL) {
    return;
  }

  EventTypeSpec event_type = {
      .eventClass = kEventClassKeyboard,
      .eventKind = kEventHotKeyPressed,
  };

  if (rust_ghostty_focus_toggle_handler_ref == NULL) {
    InstallApplicationEventHandler(
        rust_ghostty_focus_toggle_hotkey_handler,
        1,
        &event_type,
        NULL,
        &rust_ghostty_focus_toggle_handler_ref);
  }

  EventHotKeyID hotkey_id = {
      .signature = 'EGTY',
      .id = 1,
  };

  RegisterEventHotKey(
      kVK_ANSI_O,
      cmdKey | optionKey | shiftKey,
      hotkey_id,
      GetApplicationEventTarget(),
      0,
      &rust_ghostty_focus_toggle_hotkey_ref);
}

void rust_ghostty_disable_system_hide_shortcuts(void) {
  if (![NSThread isMainThread]) {
    return;
  }

  NSMenu *main_menu = [NSApp mainMenu];
  if (main_menu == nil) {
    return;
  }

  rust_ghostty_strip_hide_shortcuts_from_menu(main_menu);
}
