pub(crate) fn is_unitless_number(property_name: &str) -> bool {
    // Keep in sync with React's numeric inline-style unit behavior:
    // https://github.com/facebook/react/blob/main/packages/react-dom-bindings/src/shared/isUnitlessNumber.js
    property_name.starts_with("--")
        || matches!(
            property_name,
            "animationIterationCount"
                | "aspectRatio"
                | "borderImageOutset"
                | "borderImageSlice"
                | "borderImageWidth"
                | "boxFlex"
                | "boxFlexGroup"
                | "boxOrdinalGroup"
                | "columnCount"
                | "flexGrow"
                | "flexPositive"
                | "flexShrink"
                | "flexNegative"
                | "flexOrder"
                | "gridRowEnd"
                | "gridRowStart"
                | "gridColumnEnd"
                | "gridColumnStart"
                | "fontWeight"
                | "lineClamp"
                | "lineHeight"
                | "opacity"
                | "order"
                | "orphans"
                | "scale"
                | "tabSize"
                | "widows"
                | "zIndex"
                | "zoom"
                | "fillOpacity"
                | "floodOpacity"
                | "stopOpacity"
                | "strokeDasharray"
                | "strokeDashoffset"
                | "strokeMiterlimit"
                | "strokeOpacity"
                | "strokeWidth"
                | "MozAnimationIterationCount"
                | "MozBoxFlex"
                | "MozBoxFlexGroup"
                | "MozBoxOrdinalGroup"
                | "MozLineClamp"
                | "msAnimationIterationCount"
                | "msFlex"
                | "msZoom"
                | "msFlexGrow"
                | "msFlexNegative"
                | "msFlexOrder"
                | "msFlexPositive"
                | "msFlexShrink"
                | "msGridColumn"
                | "msGridRow"
                | "WebkitAnimationIterationCount"
                | "WebkitBoxFlex"
                | "WebkitBoxFlexGroup"
                | "WebkitBoxOrdinalGroup"
                | "WebkitColumnCount"
                | "WebkitColumns"
                | "WebkitFlex"
                | "WebkitFlexGrow"
                | "WebkitFlexPositive"
                | "WebkitFlexShrink"
                | "WebkitLineClamp"
        )
}

pub(crate) fn is_shorthand_property(property_name: &str) -> bool {
    // Cross-check against MDN's CSS property data when updating this denylist:
    // https://github.com/mdn/data/blob/main/css/properties.json
    // https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties
    matches!(
        property_name,
        "all"
            | "animation"
            | "animationRange"
            | "background"
            | "backgroundPosition"
            | "border"
            | "borderBlock"
            | "borderBlockColor"
            | "borderBlockEnd"
            | "borderBlockStart"
            | "borderBlockStyle"
            | "borderBlockWidth"
            | "borderBottom"
            | "borderColor"
            | "borderImage"
            | "borderInline"
            | "borderInlineColor"
            | "borderInlineEnd"
            | "borderInlineStart"
            | "borderInlineStyle"
            | "borderInlineWidth"
            | "borderLeft"
            | "borderRadius"
            | "borderRight"
            | "borderStyle"
            | "borderTop"
            | "borderWidth"
            | "columnRule"
            | "columns"
            | "containIntrinsicSize"
            | "caret"
            | "container"
            | "cornerShape"
            | "flex"
            | "flexFlow"
            | "font"
            | "fontSynthesis"
            | "fontVariant"
            | "gap"
            | "grid"
            | "gridArea"
            | "gridColumn"
            | "gridRow"
            | "gridTemplate"
            | "inset"
            | "insetBlock"
            | "insetInline"
            | "listStyle"
            | "margin"
            | "marginBlock"
            | "marginInline"
            | "marker"
            | "mask"
            | "maskBorder"
            | "offset"
            | "outline"
            | "overflow"
            | "overscrollBehavior"
            | "padding"
            | "paddingBlock"
            | "paddingInline"
            | "placeContent"
            | "placeItems"
            | "placeSelf"
            | "scrollMargin"
            | "scrollMarginBlock"
            | "scrollMarginInline"
            | "scrollPadding"
            | "scrollPaddingBlock"
            | "scrollPaddingInline"
            | "scrollTimeline"
            | "stroke"
            | "textDecoration"
            | "textEmphasis"
            | "textWrap"
            | "transition"
    )
}

#[cfg(test)]
mod tests {
    use super::{is_shorthand_property, is_unitless_number};

    #[test]
    fn recognizes_webkit_box_flex_group_as_unitless() {
        assert!(is_unitless_number("WebkitBoxFlexGroup"));
    }

    #[test]
    fn recognizes_aggregate_css_properties_as_shorthand() {
        for property_name in [
            "animationRange",
            "backgroundPosition",
            "caret",
            "container",
            "cornerShape",
            "marker",
            "maskBorder",
            "scrollTimeline",
            "stroke",
            "textEmphasis",
            "textWrap",
        ] {
            assert!(is_shorthand_property(property_name));
        }
    }
}
