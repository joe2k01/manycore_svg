use std::{
    cmp::{max, min},
    collections::BTreeMap,
    fmt::Display,
};

use manycore_parser::{Directions, WithID, WithXMLAttributes, COORDINATES_KEY, ID_KEY};

use super::{ProcessingInformation, TextInformation, OFFSET_FROM_BORDER, TEXT_GROUP_FILTER};
use crate::{
    ConnectionType, ConnectionsParentGroup, CoordinateT, DirectionType, FieldConfiguration,
    SVGError, SVGErrorKind, DEFAULT_FONT_SIZE,
};

pub(crate) static FONT_SIZE_WITH_OFFSET: CoordinateT = 18;

/// Binary search to fit input value in one of the 4 boundaries.
pub(crate) fn binary_search_left_insertion_point(bounds: &[u64; 4], val: u64) -> usize {
    // Bounds has always length 4
    let mut l: i8 = 0;
    let max_i: i8 = 3;
    let mut r: i8 = max_i;

    while l <= r {
        let m = l + (r - l) / 2;
        let cmp = bounds[m as usize];

        if cmp >= val {
            r = m - 1;
        } else {
            l = m + 1
        }
    }

    // We could go out of bounds, but that's meaningless for us. Constrain between 0 and 3
    let corrected_l = max(min(l, max_i), 0) as usize;

    // We found the left most insertion point
    // But we don't know if we are here because we are the same as the next element
    // or greater than the previous but smaller than next
    if corrected_l > 0 && bounds[corrected_l] > val {
        corrected_l - 1
    } else {
        corrected_l
    }
}

/// Generates [`InformationLayer`] content for a [`WithID`] element.
pub(crate) fn generate_with_id<K: Display, T: WithID<K> + WithXMLAttributes>(
    mut base_x: CoordinateT,
    mut base_y: CoordinateT,
    configuration: &BTreeMap<String, FieldConfiguration>,
    target: &T,
    group: &mut ProcessingInformation,
    text_anchor: &'static str,
    css: &mut String,
) {
    // Start by adding some padding between text and element border
    base_x = base_x.saturating_add(OFFSET_FROM_BORDER);
    base_y = base_y.saturating_add(OFFSET_FROM_BORDER);

    // ID value is outside of attributes map
    if let Some(configuration) = configuration.get(ID_KEY) {
        match configuration {
            FieldConfiguration::Text(title) => {
                group.information.push(TextInformation::new(
                    base_x,
                    base_y,
                    DEFAULT_FONT_SIZE,
                    text_anchor,
                    "text-before-edge",
                    None,
                    None,
                    format!("{}: {}", title, target.id()),
                ));
                base_y = base_y.saturating_add(FONT_SIZE_WITH_OFFSET);
            }
            _ => {
                // ID should only ever be Text.
            }
        }
    }

    // Can we even do this? i.e. does the element have attributes?
    if let Some(map) = target.other_attributes() {
        // Iterate through the requested attributes.
        for k in configuration.keys() {
            match k.as_str() {
                id_coordinates if id_coordinates == ID_KEY || id_coordinates == COORDINATES_KEY => {
                    // These have been handled
                }
                valid_key => {
                    // Fetch attribute value and its requested configuration
                    if let (Some(field_configuration), Some(value)) =
                        (configuration.get(valid_key), map.get(k))
                    {
                        match field_configuration {
                            FieldConfiguration::Text(title) => {
                                // Simple Text
                                group.information.push(TextInformation::new(
                                    base_x,
                                    base_y,
                                    DEFAULT_FONT_SIZE,
                                    text_anchor,
                                    "text-before-edge",
                                    None,
                                    None,
                                    format!("{}: {}", title, value),
                                ));

                                // Increase y for next element, if any
                                base_y = base_y.saturating_add(FONT_SIZE_WITH_OFFSET);
                            }
                            FieldConfiguration::Fill(colour_config) => {
                                // Fill colour
                                let bounds = colour_config.bounds();

                                // If we can't parse it as a number, we can't calculate what the fill colour should be.
                                // TODO: Conversion error instead?
                                if let Ok(value_num) = value.parse::<u64>() {
                                    let fill_idx =
                                        binary_search_left_insertion_point(bounds, value_num);

                                    // Add fill colour in the [`SVG`] CSS
                                    css.push_str(
                                        format!(
                                            "\n#{}{} {{fill: {};}}",
                                            target.variant(),
                                            target.id(),
                                            colour_config.colours()[fill_idx]
                                        )
                                        .as_str(),
                                    );

                                    // If we have a fill, then we need to add some background for any text element.
                                    group.filter = Some(TEXT_GROUP_FILTER);
                                }
                            }
                            FieldConfiguration::ColouredText(title, colour_config) => {
                                // Coloured text
                                let fill = get_attribute_colour(
                                    colour_config.bounds(),
                                    colour_config.colours(),
                                    value,
                                );

                                group.information.push(TextInformation::new(
                                    base_x,
                                    base_y,
                                    DEFAULT_FONT_SIZE,
                                    text_anchor,
                                    "text-before-edge",
                                    fill,
                                    None,
                                    format!("{}: {}", title, value),
                                ));

                                // Increase y for next element, if any
                                base_y = base_y.saturating_add(FONT_SIZE_WITH_OFFSET);
                            }
                            _ => {
                                // Remaining variants are handled elsewhere/for other elements
                            }
                        }
                    } // else this element does not contain this attribute
                }
            }
        }
    }
}

/// Calculates the corresponding colour for an attribute value given some bounds.
pub(crate) fn get_attribute_colour<'a>(
    bounds: &'a [u64; 4],
    colours: &'a [String; 4],
    attribute_value: &'a String,
) -> Option<&'a String> {
    let mut fill: Option<&String> = None;

    // TODO: Conversion errorr instead?
    if let Ok(value_num) = attribute_value.parse::<u64>() {
        let fill_idx = binary_search_left_insertion_point(bounds, value_num);
        fill = Some(&colours[fill_idx]);
    }

    fill
}

/// Determines the type of an SVG connection: Input/Output.
pub(crate) fn get_connection_type<'a>(
    connections_group: &'a ConnectionsParentGroup,
    direction_type: &'a DirectionType,
    core_id: &'a u8,
) -> Result<&'a ConnectionType, SVGError> {
    connections_group
        .core_connections_map()
        .get(core_id)
        .ok_or(SVGError::new(SVGErrorKind::ConnectionError(format!(
            "Could not get connections for Core {}",
            core_id
        ))))?
        .get(direction_type)
        .ok_or(SVGError::new(SVGErrorKind::ConnectionError(format!(
            "Could not get connection {} for Core {}",
            direction_type, core_id
        ))))
}

/// Wrapper to generate error when we can't grab an SVG connection.
pub(crate) fn missing_connection(idx: &usize) -> SVGError {
    SVGError::new(SVGErrorKind::ConnectionError(format!(
        "Could not grab SVG connection path for Core {}",
        idx
    )))
}

/// Wrapper to generate error when we expected a channel and did not find one.
pub(crate) fn missing_channel(core_id: &u8, direction: &Directions) -> SVGError {
    SVGError::new(SVGErrorKind::ManycoreMismatch(format!(
        "Could not retrieve {} channel for Core {}",
        direction, core_id
    )))
}

/// Wrapper to generate error when we expected source loads and did not find any.
pub(crate) fn missing_source_loads(core_id: &u8) -> SVGError {
    SVGError::new(SVGErrorKind::ManycoreMismatch(format!(
        "Could not retrieve source loads for Core {}",
        core_id
    )))
}

/// Wrapper to generate error when we expected a source channel load and did not find one.
pub(crate) fn missing_source_load(core_id: &u8, direction: &Directions) -> SVGError {
    SVGError::new(SVGErrorKind::ManycoreMismatch(format!(
        "Could not retrieve {} source channel load for Core {}",
        direction, core_id
    )))
}
