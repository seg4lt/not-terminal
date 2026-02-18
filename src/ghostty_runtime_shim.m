#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdatomic.h>
#include <stdlib.h>

#import <AppKit/AppKit.h>

#include "../vendor/ghostty/include/ghostty.h"

typedef struct rust_ghostty_runtime_state_s {
  atomic_bool pending_tick;
} rust_ghostty_runtime_state_t;

typedef struct rust_ghostty_runtime_bundle_s {
  rust_ghostty_runtime_state_t *state;
  ghostty_runtime_config_s config;
} rust_ghostty_runtime_bundle_t;

static void rust_ghostty_wakeup_cb(void *userdata) {
  rust_ghostty_runtime_state_t *state = (rust_ghostty_runtime_state_t *)userdata;
  if (state == NULL) {
    return;
  }

  atomic_store_explicit(&state->pending_tick, true, memory_order_release);
}

static bool rust_ghostty_action_cb(ghostty_app_t app,
                                   ghostty_target_s target,
                                   ghostty_action_s action) {
  (void)app;
  (void)target;
  (void)action;
  return false;
}

static void rust_ghostty_read_clipboard_cb(void *userdata,
                                           ghostty_clipboard_e location,
                                           void *state) {
  (void)userdata;
  (void)location;
  (void)state;
}

static void rust_ghostty_confirm_read_clipboard_cb(
    void *userdata,
    const char *value,
    void *state,
    ghostty_clipboard_request_e request) {
  (void)userdata;
  (void)value;
  (void)state;
  (void)request;
}

static void rust_ghostty_write_clipboard_cb(
    void *userdata,
    ghostty_clipboard_e location,
    const ghostty_clipboard_content_s *content,
    size_t len,
    bool requires_confirmation) {
  (void)userdata;
  (void)location;
  (void)content;
  (void)len;
  (void)requires_confirmation;
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

  rust_ghostty_runtime_bundle_t *bundle =
      (rust_ghostty_runtime_bundle_t *)malloc(sizeof(rust_ghostty_runtime_bundle_t));
  if (bundle == NULL) {
    free(state);
    return NULL;
  }

  bundle->state = state;
  bundle->config.userdata = state;
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

typedef void *(*rust_ghostty_surface_new_fn)(void *, const ghostty_surface_config_s *);

void *rust_ghostty_surface_new_macos(void *surface_new_fn_raw,
                                     void *app,
                                     void *ns_view,
                                     double scale_factor,
                                     float font_size_points,
                                     const char *working_directory) {
  if (surface_new_fn_raw == NULL || app == NULL || ns_view == NULL) {
    return NULL;
  }

  rust_ghostty_surface_new_fn surface_new_fn =
      (rust_ghostty_surface_new_fn)surface_new_fn_raw;

  ghostty_surface_config_s config = {0};
  config.platform_tag = GHOSTTY_PLATFORM_MACOS;
  config.platform.macos.nsview = ns_view;
  config.scale_factor = scale_factor;
  config.font_size = font_size_points;
  config.working_directory = working_directory;
  config.context = GHOSTTY_SURFACE_CONTEXT_WINDOW;

  return surface_new_fn(app, &config);
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
