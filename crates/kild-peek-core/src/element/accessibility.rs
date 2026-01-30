use std::ffi::c_void;
use std::ptr;

use accessibility_sys::{
    AXError, AXUIElementCopyAttributeValue, AXUIElementCreateApplication, AXUIElementRef,
    AXUIElementSetMessagingTimeout, AXValueGetValue, AXValueRef, kAXChildrenAttribute,
    kAXDescriptionAttribute, kAXEnabledAttribute, kAXErrorAttributeUnsupported, kAXErrorNoValue,
    kAXErrorSuccess, kAXPositionAttribute, kAXRoleAttribute, kAXSizeAttribute, kAXTitleAttribute,
    kAXValueAttribute, kAXValueTypeCGPoint, kAXValueTypeCGSize, kAXWindowsAttribute,
};
use core_foundation::array::CFArray;
use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::string::CFString;
use tracing::debug;

/// Maximum depth for recursive element tree traversal
const MAX_TRAVERSAL_DEPTH: usize = 20;

/// Timeout for AX messaging (seconds)
const AX_MESSAGING_TIMEOUT: f32 = 1.0;

/// Raw element data from the Accessibility API
#[derive(Debug, Clone)]
pub struct RawElement {
    role: String,
    title: Option<String>,
    value: Option<String>,
    description: Option<String>,
    /// Screen-absolute position
    position: Option<(f64, f64)>,
    /// Element size
    size: Option<(f64, f64)>,
    enabled: bool,
}

impl RawElement {
    /// Create a new RawElement. Internal use only.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        role: String,
        title: Option<String>,
        value: Option<String>,
        description: Option<String>,
        position: Option<(f64, f64)>,
        size: Option<(f64, f64)>,
        enabled: bool,
    ) -> Self {
        Self {
            role,
            title,
            value,
            description,
            position,
            size,
            enabled,
        }
    }

    pub fn role(&self) -> &str {
        &self.role
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn position(&self) -> Option<(f64, f64)> {
        self.position
    }

    pub fn size(&self) -> Option<(f64, f64)> {
        self.size
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }
}

/// Query all UI elements from an application by PID
///
/// Traverses the AX element tree starting from the app's windows,
/// collecting all elements up to MAX_TRAVERSAL_DEPTH levels deep.
pub fn query_elements(pid: i32) -> Result<Vec<RawElement>, String> {
    // SAFETY: AXUIElementCreateApplication is a well-defined FFI function that creates
    // an AXUIElementRef for the given PID. The returned ref is owned by us.
    let app_element = unsafe { AXUIElementCreateApplication(pid) };
    if app_element.is_null() {
        return Err(format!("Failed to create AX element for PID {}", pid));
    }

    // Set messaging timeout to avoid hangs on unresponsive apps
    // SAFETY: app_element is a valid AXUIElementRef we just created.
    unsafe {
        AXUIElementSetMessagingTimeout(app_element, AX_MESSAGING_TIMEOUT);
    }

    // Get all windows from the application
    let (window_refs, _window_array) = get_children_refs(app_element, kAXWindowsAttribute);

    let mut elements = Vec::new();

    for window_element in &window_refs {
        collect_elements(*window_element, &mut elements, 0);
    }

    // SAFETY: AXUIElementCreateApplication follows the Create Rule, returning a +1 retained
    // reference that we own. Unlike CFString or CFArray, AXUIElementRef has no Rust TCFType
    // wrapper for RAII, so we must manually call CFRelease. Failing to release would leak memory.
    unsafe {
        core_foundation::base::CFRelease(app_element as *mut c_void);
    }

    Ok(elements)
}

/// Recursively collect elements from the AX tree
fn collect_elements(element: AXUIElementRef, out: &mut Vec<RawElement>, depth: usize) {
    if depth > MAX_TRAVERSAL_DEPTH {
        return;
    }

    // Read this element's properties
    match read_element_properties(element) {
        Some(raw) => out.push(raw),
        None => {
            debug!(
                event = "peek.core.element.property_read_skipped",
                depth = depth
            );
        }
    }

    // Recurse into children
    let (child_refs, _children_array) = get_children_refs(element, kAXChildrenAttribute);
    for child_element in &child_refs {
        collect_elements(*child_element, out, depth + 1);
    }
}

/// Read all properties from a single AX element
fn read_element_properties(element: AXUIElementRef) -> Option<RawElement> {
    let role = get_string_attribute(element, kAXRoleAttribute)?;

    let title = get_string_attribute(element, kAXTitleAttribute);
    let value = get_string_attribute(element, kAXValueAttribute);
    let description = get_string_attribute(element, kAXDescriptionAttribute);
    let position = get_position(element);
    let size = get_size(element);
    let enabled = get_boolean_attribute(element, kAXEnabledAttribute).unwrap_or_else(|| {
        debug!(
            event = "peek.core.element.enabled_default_fallback",
            message = "enabled attribute unavailable, defaulting to true"
        );
        true
    });

    Some(RawElement::new(
        role,
        title,
        value,
        description,
        position,
        size,
        enabled,
    ))
}

/// Get a string attribute from an AX element
fn get_string_attribute(element: AXUIElementRef, attribute: &str) -> Option<String> {
    let cf_attr = CFString::new(attribute);
    let mut value: core_foundation::base::CFTypeRef = ptr::null();

    // SAFETY: element is a valid AXUIElementRef. We pass a valid CFStringRef and
    // a pointer to receive the value. On success, value is a retained CFTypeRef.
    let result = unsafe {
        AXUIElementCopyAttributeValue(element, cf_attr.as_concrete_TypeRef(), &mut value)
    };

    if result != kAXErrorSuccess || value.is_null() {
        if result != kAXErrorNoValue as AXError && result != kAXErrorAttributeUnsupported as AXError
        {
            debug!(
                event = "peek.core.element.attribute_read_failed",
                attribute = attribute,
                error_code = result
            );
        }
        return None;
    }

    // SAFETY: value is a retained CFTypeRef from CopyAttributeValue (Create Rule).
    // wrap_under_create_rule takes ownership so it will be released when dropped.
    let cf_type: CFType = unsafe { TCFType::wrap_under_create_rule(value) };

    if cf_type.instance_of::<CFString>() {
        // Extract the string before cf_type drops
        let ptr = cf_type.as_CFTypeRef() as *const _;
        let s = unsafe { CFString::wrap_under_get_rule(ptr) }.to_string();
        Some(s)
    } else {
        None
    }
}

/// Get a boolean attribute from an AX element
fn get_boolean_attribute(element: AXUIElementRef, attribute: &str) -> Option<bool> {
    let cf_attr = CFString::new(attribute);
    let mut value: core_foundation::base::CFTypeRef = ptr::null();

    // SAFETY: Same pattern as get_string_attribute.
    let result = unsafe {
        AXUIElementCopyAttributeValue(element, cf_attr.as_concrete_TypeRef(), &mut value)
    };

    if result != kAXErrorSuccess || value.is_null() {
        if result != kAXErrorNoValue as AXError && result != kAXErrorAttributeUnsupported as AXError
        {
            debug!(
                event = "peek.core.element.attribute_read_failed",
                attribute = attribute,
                error_code = result
            );
        }
        return None;
    }

    // SAFETY: value is a retained CFTypeRef from CopyAttributeValue (Create Rule).
    // wrap_under_create_rule takes ownership so it will be released when dropped.
    let cf_type: CFType = unsafe { TCFType::wrap_under_create_rule(value) };

    if cf_type.instance_of::<CFBoolean>() {
        let ptr = cf_type.as_CFTypeRef() as *const _;
        let b: bool = unsafe { CFBoolean::wrap_under_get_rule(ptr) }.into();
        Some(b)
    } else {
        None
    }
}

/// Get the position (CGPoint) from an AX element
fn get_position(element: AXUIElementRef) -> Option<(f64, f64)> {
    get_ax_value_point(element, kAXPositionAttribute)
}

/// Get the size (CGSize) from an AX element
fn get_size(element: AXUIElementRef) -> Option<(f64, f64)> {
    get_ax_value_size(element, kAXSizeAttribute)
}

/// Extract a CGPoint from an AXValue attribute
fn get_ax_value_point(element: AXUIElementRef, attribute: &str) -> Option<(f64, f64)> {
    let cf_attr = CFString::new(attribute);
    let mut value: core_foundation::base::CFTypeRef = ptr::null();

    // SAFETY: Standard AXUIElementCopyAttributeValue call (Copy Rule: +1 retained ref).
    let result = unsafe {
        AXUIElementCopyAttributeValue(element, cf_attr.as_concrete_TypeRef(), &mut value)
    };

    if result != kAXErrorSuccess || value.is_null() {
        if result != kAXErrorNoValue as AXError && result != kAXErrorAttributeUnsupported as AXError
        {
            debug!(
                event = "peek.core.element.attribute_read_failed",
                attribute = attribute,
                error_code = result
            );
        }
        return None;
    }

    // SAFETY: value is an AXValueRef containing a CGPoint.
    let ax_value = value as AXValueRef;
    let mut point = core_graphics::geometry::CGPoint::new(0.0, 0.0);

    // SAFETY: AXValueGetValue reads the CGPoint from the AXValue.
    let ok = unsafe {
        AXValueGetValue(
            ax_value,
            kAXValueTypeCGPoint,
            &mut point as *mut _ as *mut c_void,
        )
    };

    // SAFETY: Release the value we got from CopyAttributeValue.
    unsafe {
        core_foundation::base::CFRelease(value as *mut c_void);
    }

    if ok { Some((point.x, point.y)) } else { None }
}

/// Extract a CGSize from an AXValue attribute
fn get_ax_value_size(element: AXUIElementRef, attribute: &str) -> Option<(f64, f64)> {
    let cf_attr = CFString::new(attribute);
    let mut value: core_foundation::base::CFTypeRef = ptr::null();

    // SAFETY: Standard AXUIElementCopyAttributeValue call (Copy Rule: +1 retained ref).
    let result = unsafe {
        AXUIElementCopyAttributeValue(element, cf_attr.as_concrete_TypeRef(), &mut value)
    };

    if result != kAXErrorSuccess || value.is_null() {
        if result != kAXErrorNoValue as AXError && result != kAXErrorAttributeUnsupported as AXError
        {
            debug!(
                event = "peek.core.element.attribute_read_failed",
                attribute = attribute,
                error_code = result
            );
        }
        return None;
    }

    // SAFETY: value is an AXValueRef containing a CGSize.
    let ax_value = value as AXValueRef;
    let mut size = core_graphics::geometry::CGSize::new(0.0, 0.0);

    // SAFETY: AXValueGetValue reads the CGSize from the AXValue.
    let ok = unsafe {
        AXValueGetValue(
            ax_value,
            kAXValueTypeCGSize,
            &mut size as *mut _ as *mut c_void,
        )
    };

    // SAFETY: Release the value we got from CopyAttributeValue.
    unsafe {
        core_foundation::base::CFRelease(value as *mut c_void);
    }

    if ok {
        Some((size.width, size.height))
    } else {
        None
    }
}

/// Get children (or windows) array from an AX element.
/// Returns raw AXUIElementRef pointers and the backing CFArray.
///
/// # Safety Contract
///
/// The returned `AXUIElementRef` pointers are **unretained borrows** into the CFArray's
/// internal storage. The CFArray owns these references and will release them when dropped.
///
/// **Caller responsibilities:**
/// - Keep the returned `Option<CFArray<CFType>>` alive for as long as any pointer is used
/// - Do NOT call `CFRelease` on individual element refs (would cause double-free)
/// - Do NOT store pointers beyond the CFArray's lifetime
fn get_children_refs(
    element: AXUIElementRef,
    attribute: &str,
) -> (Vec<AXUIElementRef>, Option<CFArray<CFType>>) {
    let cf_attr = CFString::new(attribute);
    let mut value: core_foundation::base::CFTypeRef = ptr::null();

    // SAFETY: AXUIElementCopyAttributeValue follows the Copy Rule, returning a +1 retained
    // CFTypeRef on success. We must either transfer ownership to an RAII wrapper or manually
    // release the value to avoid leaking memory.
    let result = unsafe {
        AXUIElementCopyAttributeValue(element, cf_attr.as_concrete_TypeRef(), &mut value)
    };

    if result != kAXErrorSuccess || value.is_null() {
        if result != kAXErrorNoValue as AXError && result != kAXErrorAttributeUnsupported as AXError
        {
            debug!(
                event = "peek.core.element.attribute_read_failed",
                attribute = attribute,
                error_code = result
            );
        }
        return (Vec::new(), None);
    }

    // SAFETY: value is a +1 retained CFArrayRef from CopyAttributeValue (Copy Rule).
    // wrap_under_create_rule transfers ownership to the Rust wrapper, which will call
    // CFRelease when dropped.
    let cf_array: CFArray<CFType> =
        unsafe { CFArray::wrap_under_create_rule(value as core_foundation::array::CFArrayRef) };

    // SAFETY: The AXUIElementRef pointers extracted here are borrowed references into the
    // CFArray's internal storage. They are NOT retained — the CFArray owns them. Callers
    // must NOT release these refs (would cause double-free when CFArray drops). The refs
    // are only valid while the returned CFArray remains alive.
    let refs: Vec<AXUIElementRef> = cf_array
        .iter()
        .map(|item| item.as_CFTypeRef() as AXUIElementRef)
        .collect();

    // Return the array alongside refs so caller keeps the CFArray alive for ref validity
    (refs, Some(cf_array))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_element_debug() {
        let elem = RawElement::new(
            "AXButton".to_string(),
            Some("OK".to_string()),
            None,
            None,
            Some((100.0, 200.0)),
            Some((80.0, 30.0)),
            true,
        );
        let debug_str = format!("{:?}", elem);
        assert!(debug_str.contains("AXButton"));
        assert!(debug_str.contains("OK"));
    }

    #[test]
    fn test_raw_element_clone() {
        let elem = RawElement::new(
            "AXTextField".to_string(),
            None,
            Some("hello".to_string()),
            Some("text input".to_string()),
            None,
            None,
            false,
        );
        let cloned = elem.clone();
        assert_eq!(cloned.role(), "AXTextField");
        assert_eq!(cloned.value(), Some("hello"));
        assert!(!cloned.enabled());
    }

    #[test]
    fn test_raw_element_getters() {
        let elem = RawElement::new(
            "AXButton".to_string(),
            Some("Submit".to_string()),
            Some("value".to_string()),
            Some("desc".to_string()),
            Some((10.0, 20.0)),
            Some((100.0, 50.0)),
            true,
        );
        assert_eq!(elem.role(), "AXButton");
        assert_eq!(elem.title(), Some("Submit"));
        assert_eq!(elem.value(), Some("value"));
        assert_eq!(elem.description(), Some("desc"));
        assert_eq!(elem.position(), Some((10.0, 20.0)));
        assert_eq!(elem.size(), Some((100.0, 50.0)));
        assert!(elem.enabled());
    }

    #[test]
    fn test_max_traversal_depth_constant() {
        assert_eq!(MAX_TRAVERSAL_DEPTH, 20);
    }

    #[test]
    fn test_ax_messaging_timeout_constant() {
        assert!((AX_MESSAGING_TIMEOUT - 1.0).abs() < f32::EPSILON);
    }

    // Integration tests that require accessibility permissions
    #[test]
    #[ignore]
    fn test_query_elements_requires_permission() {
        // This test requires accessibility permission and a running app
        // Run manually: cargo test --all -- --ignored test_query_elements
        let result = query_elements(1); // PID 1 (launchd) - should return something or error
        // Just verify it doesn't panic
        let _ = result;
    }
}
