// FFI bindings for native WebView on macOS
use std::ffi::CStr;
use std::ptr::NonNull;

// Link to the Objective-C shim (compiled together with ghostty_runtime_shim)
#[link(name = "ghostty_runtime_shim", kind = "static")]
unsafe extern "C" {
    fn webview_new(parent_ns_view: usize) -> *mut ();
    fn webview_free(webview_ptr: *mut ());
    fn webview_load_url(webview_ptr: *mut (), url_cstr: *const i8);
    fn webview_load_html(webview_ptr: *mut (), html_cstr: *const i8);
    fn webview_go_back(webview_ptr: *mut ());
    fn webview_go_forward(webview_ptr: *mut ());
    fn webview_reload(webview_ptr: *mut ());
    fn webview_set_frame(webview_ptr: *mut (), x: f64, y: f64, width: f64, height: f64);
    fn webview_set_hidden(webview_ptr: *mut (), hidden: bool);
    fn webview_can_go_back(webview_ptr: *mut ()) -> bool;
    fn webview_can_go_forward(webview_ptr: *mut ()) -> bool;
    #[allow(dead_code)]
    fn webview_get_url(webview_ptr: *mut ()) -> *mut i8;
    #[allow(dead_code)]
    fn webview_get_title(webview_ptr: *mut ()) -> *mut i8;
    fn webview_open_dev_tools(webview_ptr: *mut ());
    #[allow(dead_code)]
    fn webview_lose_focus(webview_ptr: *mut ());
    #[allow(dead_code)]
    fn free(ptr: *mut std::ffi::c_void);
}

/// Wrapper for the native webview handle
pub struct WebView {
    ptr: NonNull<()>,
}

unsafe impl Send for WebView {}
unsafe impl Sync for WebView {}

impl WebView {
    /// Create a hosted webview from a raw NSView pointer from the app runtime.
    pub fn new_hosted(parent_ns_view: usize) -> Option<Self> {
        if parent_ns_view == 0 {
            return None;
        }

        // SAFETY: `parent_ns_view` comes from AppKit window resolution and is validated non-zero above.
        unsafe { Self::new(parent_ns_view) }
    }

    /// Create a new webview hosted in the parent NSView
    ///
    /// # Safety
    /// The parent_ns_view must be a valid pointer to an NSView
    pub unsafe fn new(parent_ns_view: usize) -> Option<Self> {
        let ptr = unsafe { webview_new(parent_ns_view) };
        NonNull::new(ptr).map(|ptr| Self { ptr })
    }

    /// Load a URL in the webview
    pub fn load_url(&self, url: &str) {
        // Use a small scope for the CString to ensure it lives long enough
        let Ok(url_cstr) = std::ffi::CString::new(url) else {
            return;
        };
        unsafe {
            webview_load_url(self.ptr.as_ptr(), url_cstr.as_ptr());
        }
    }

    /// Load inline HTML into the webview.
    pub fn load_html(&self, html: &str) {
        let Ok(html_cstr) = std::ffi::CString::new(html) else {
            return;
        };
        unsafe {
            webview_load_html(self.ptr.as_ptr(), html_cstr.as_ptr());
        }
    }

    /// Navigate back in history
    pub fn go_back(&self) {
        unsafe {
            webview_go_back(self.ptr.as_ptr());
        }
    }

    /// Navigate forward in history
    pub fn go_forward(&self) {
        unsafe {
            webview_go_forward(self.ptr.as_ptr());
        }
    }

    /// Reload the current page
    pub fn reload(&self) {
        unsafe {
            webview_reload(self.ptr.as_ptr());
        }
    }

    /// Update the webview's frame/size
    pub fn set_frame(&self, x: f64, y: f64, width: f64, height: f64) {
        unsafe {
            webview_set_frame(self.ptr.as_ptr(), x, y, width, height);
        }
    }

    /// Set container visibility
    pub fn set_hidden(&self, hidden: bool) {
        unsafe {
            webview_set_hidden(self.ptr.as_ptr(), hidden);
        }
    }

    /// Check if can go back
    pub fn can_go_back(&self) -> bool {
        unsafe { webview_can_go_back(self.ptr.as_ptr()) }
    }

    /// Check if can go forward
    pub fn can_go_forward(&self) -> bool {
        unsafe { webview_can_go_forward(self.ptr.as_ptr()) }
    }

    /// Get current URL
    #[allow(dead_code)]
    pub fn get_url(&self) -> Option<String> {
        unsafe {
            let ptr = webview_get_url(self.ptr.as_ptr());
            if ptr.is_null() {
                return None;
            }
            let cstr = CStr::from_ptr(ptr);
            let result = cstr.to_string_lossy().to_string();
            // Free the string allocated by Objective-C
            free(ptr as *mut std::ffi::c_void);
            Some(result)
        }
    }

    /// Get current page title
    #[allow(dead_code)]
    pub fn get_title(&self) -> Option<String> {
        unsafe {
            let ptr = webview_get_title(self.ptr.as_ptr());
            if ptr.is_null() {
                return None;
            }
            let cstr = CStr::from_ptr(ptr);
            let result = cstr.to_string_lossy().to_string();
            // Free the string allocated by Objective-C
            free(ptr as *mut std::ffi::c_void);
            Some(result)
        }
    }

    /// Open WebKit Inspector/DevTools
    pub fn open_dev_tools(&self) {
        unsafe {
            webview_open_dev_tools(self.ptr.as_ptr());
        }
    }

    /// Make the webview lose focus
    #[allow(dead_code)]
    pub fn lose_focus(&self) {
        unsafe {
            webview_lose_focus(self.ptr.as_ptr());
        }
    }
}

impl Drop for WebView {
    fn drop(&mut self) {
        unsafe {
            webview_free(self.ptr.as_ptr());
        }
    }
}
