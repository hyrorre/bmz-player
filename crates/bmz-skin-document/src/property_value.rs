use super::*;

/// A typed beatoraja property: a numeric ID, a property name, or a Lua expression.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum SkinPropertyValue {
    Id(i32),
    Expression(String),
}

impl SkinPropertyValue {
    pub fn resolve_id(&self, family: PropertyFamily) -> Option<i32> {
        match self {
            Self::Id(id) => Some(*id),
            Self::Expression(name) => resolve_property_name(family, name),
        }
    }

    pub fn expression(&self) -> Option<&str> {
        match self {
            Self::Id(_) => None,
            Self::Expression(expression) => Some(expression),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn typed_properties_preserve_names_numeric_ids_and_unknown_expressions() {
        let document: SkinDocument = serde_json::from_value(json!({
            "value": [{"id":"score", "value":"score"}, {"value":71}, {"value":"number(71) + 1"}],
            "imageset": [{"ref":330, "value":"score", "images":[]}],
            "text": [{"value":"title"}],
            "slider": [{"value":"music_progress"}],
            "graph": [{"value":"score_rate"}]
        }))
        .unwrap();
        assert_eq!(
            document.value[0].value.as_ref().unwrap().resolve_id(PropertyFamily::Integer),
            Some(71)
        );
        assert_eq!(document.value[1].value, Some(SkinPropertyValue::Id(71)));
        assert_eq!(document.value[2].value.as_ref().unwrap().expression(), Some("number(71) + 1"));
        assert_eq!(document.imageset[0].ref_id, 330);
        assert_eq!(document.imageset[0].value, document.value[0].value);
        assert_eq!(
            document.text[0].value.as_ref().unwrap().resolve_id(PropertyFamily::String),
            Some(10)
        );
        assert_eq!(
            document.slider[0].value.as_ref().unwrap().resolve_id(PropertyFamily::Rate),
            Some(6)
        );
        let float = document.graph[0].value.as_ref().unwrap();
        assert_eq!(float.resolve_id(PropertyFamily::Float), Some(1102));
        assert_eq!(float.resolve_id(PropertyFamily::Rate), None);
    }

    #[test]
    fn destination_names_and_expressions_keep_separate_and_conditions() {
        let destination: SkinDestinationDef = serde_json::from_value(json!({
            "op": [41, "bgaon", "!bgaoff", "!!bgaon", "!unknown", "option(40) or option(41)"],
            "op_expr": ["number(71) > 0"], "draw":"!bgaoff"
        }))
        .unwrap();
        assert_eq!(destination.op, [41, 41, -40, 41]);
        assert_eq!(
            destination.op_expr.as_ref(),
            ["number(71) > 0", "!unknown", "option(40) or option(41)"]
        );
        assert_eq!(destination.draw, "!bgaoff");
        let numeric: SkinDestinationDef =
            serde_json::from_value(json!({"op":41, "draw":41})).unwrap();
        assert_eq!(numeric.op, [41]);
        assert!(numeric.op_expr.is_empty());
        assert_eq!(numeric.draw, "option(41)");
    }

    #[test]
    fn image_refs_remain_numeric_in_the_index_namespace() {
        assert!(serde_json::from_value::<SkinImageDef>(json!({"ref":"lanecover"})).is_err());
        assert!(serde_json::from_value::<SkinImageSetDef>(json!({"ref":"lanecover"})).is_err());
        assert_eq!(resolve_property_name(PropertyFamily::Index, "lanecover"), Some(330));
        assert_eq!(resolve_property_name(PropertyFamily::Integer, "lanecover"), None);
    }
}
