#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>
#include <pthread.h>

#import <AppKit/AppKit.h>
#import <Carbon/Carbon.h>

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
} rust_ghostty_action_tag_t;

typedef struct rust_ghostty_action_event_s {
  uint32_t tag;
  uintptr_t surface;
  int32_t arg0;
  uint16_t amount;
  uint16_t reserved;
  uintptr_t ptr;  // For passing pointers (e.g., title strings)
  char title_copy[256];  // Buffer to copy title strings immediately
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
} rust_ghostty_runtime_bundle_t;

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
  if (target.tag == GHOSTTY_TARGET_SURFACE) {
    surface_ptr = (uintptr_t)target.target.surface;
  }

  rust_ghostty_action_event_t action_event = {
      .tag = RUST_GHOSTTY_ACTION_NONE,
      .surface = surface_ptr,
      .arg0 = 0,
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
        strncpy(action_event.title_copy, title, sizeof(action_event.title_copy) - 1);
        action_event.title_copy[sizeof(action_event.title_copy) - 1] = '\0';
        action_event.ptr = (uintptr_t)action_event.title_copy;
      } else {
        action_event.title_copy[0] = '\0';
        action_event.ptr = 0;
      }
      break;
    }
    case GHOSTTY_ACTION_DESKTOP_NOTIFICATION: {
      action_event.tag = RUST_GHOSTTY_ACTION_DESKTOP_NOTIFICATION;
      break;
    }
    default:
      return false;
  }

  rust_ghostty_enqueue_action(state, action_event);
  return true;
}

static void rust_ghostty_read_clipboard_cb(void *userdata,
                                           ghostty_clipboard_e location,
                                           void *state) {
  (void)location;
  // userdata is the runtime bundle (set via surface config)
  rust_ghostty_runtime_bundle_t *bundle =
      (rust_ghostty_runtime_bundle_t *)userdata;
  if (bundle == NULL) {
    return;
  }

  ghostty_surface_t surface = (ghostty_surface_t)bundle->surface;
  if (surface == NULL) {
    return;
  }

  NSPasteboard *pasteboard = [NSPasteboard generalPasteboard];
  NSString *content = [pasteboard stringForType:NSPasteboardTypeString];
  if (content == nil) {
    content = @"";
  }

  // Complete the clipboard request with the data
  ghostty_surface_complete_clipboard_request(surface, [content UTF8String], state, false);
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
}

void rust_ghostty_host_view_set_hidden(void *host_ns_view, bool hidden) {
  if (host_ns_view == NULL) {
    return;
  }

  NSView *host = (NSView *)host_ns_view;
  [host setHidden:hidden ? YES : NO];
}

void rust_ghostty_host_view_free(void *host_ns_view) {
  if (host_ns_view == NULL) {
    return;
  }

  NSView *host = (NSView *)host_ns_view;
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
