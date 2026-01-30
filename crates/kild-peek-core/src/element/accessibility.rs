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
    pub role: String,
    pub title: Option<String>,
    pub value: Option<String>,
    pub description: Option<String>,
    /// Screen-absolute position
    pub position: Option<(f64, f64)>,
    /// Element size
    pub size: Option<(f64, f64)>,
    pub enabled: bool,
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

    // SAFETY: Release the app element we created.
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
    let enabled = get_boolean_attribute(element, kAXEnabledAttribute).unwrap_or(true);

    Some(RawElement {
        role,
        title,
        value,
        description,
        position,
        size,
        enabled,
    })
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

    // SAFETY: Standard AXUIElementCopyAttributeValue call.
    let result = unsafe {
        AXUIElementCopyAttributeValue(element, cf_attr.as_concrete_TypeRef(), &mut value)
    };

    if result != kAXErrorSuccess || value.is_null() {
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

    // SAFETY: Standard AXUIElementCopyAttributeValue call.
    let result = unsafe {
        AXUIElementCopyAttributeValue(element, cf_attr.as_concrete_TypeRef(), &mut value)
    };

    if result != kAXErrorSuccess || value.is_null() {
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
/// SAFETY: The returned pointers are only valid while the CFArray (second tuple element)
/// remains alive. Caller must retain the CFArray for the lifetime of pointer usage.
/// Do NOT release the individual element refs — they are owned by the CFArray.
fn get_children_refs(
    element: AXUIElementRef,
    attribute: &str,
) -> (Vec<AXUIElementRef>, Option<CFArray<CFType>>) {
    let cf_attr = CFString::new(attribute);
    let mut value: core_foundation::base::CFTypeRef = ptr::null();

    // SAFETY: Standard AXUIElementCopyAttributeValue call.
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

    // SAFETY: value is a CFArrayRef from CopyAttributeValue. wrap_under_create_rule
    // takes ownership for proper cleanup.
    let cf_array: CFArray<CFType> =
        unsafe { CFArray::wrap_under_create_rule(value as core_foundation::array::CFArrayRef) };

    let refs: Vec<AXUIElementRef> = cf_array
        .iter()
        .map(|item| item.as_CFTypeRef() as AXUIElementRef)
        .collect();

    // Return the array alongside refs to keep it alive
    (refs, Some(cf_array))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_element_debug() {
        let elem = RawElement {
            role: "AXButton".to_string(),
            title: Some("OK".to_string()),
            value: None,
            description: None,
            position: Some((100.0, 200.0)),
            size: Some((80.0, 30.0)),
            enabled: true,
        };
        let debug_str = format!("{:?}", elem);
        assert!(debug_str.contains("AXButton"));
        assert!(debug_str.contains("OK"));
    }

    #[test]
    fn test_raw_element_clone() {
        let elem = RawElement {
            role: "AXTextField".to_string(),
            title: None,
            value: Some("hello".to_string()),
            description: Some("text input".to_string()),
            position: None,
            size: None,
            enabled: false,
        };
        let cloned = elem.clone();
        assert_eq!(cloned.role, "AXTextField");
        assert_eq!(cloned.value.as_deref(), Some("hello"));
        assert!(!cloned.enabled);
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
