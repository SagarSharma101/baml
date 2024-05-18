use anyhow::Result;

use super::python_language_features::ToPython;
use internal_baml_core::ir::{repr::IntermediateRepr, ClassWalker, EnumWalker, FieldType};

#[derive(askama::Template)]
#[template(path = "types.py.j2", escape = "none")]
pub(crate) struct PythonTypes<'ir> {
    enums: Vec<PythonEnum<'ir>>,
    classes: Vec<PythonClass<'ir>>,
}

struct PythonEnum<'ir> {
    pub name: &'ir str,
    pub values: Vec<&'ir str>,
}

struct PythonClass<'ir> {
    name: &'ir str,
    fields: Vec<(&'ir str, String)>,
}

#[derive(askama::Template)]
#[template(path = "partial_types.py.j2", escape = "none")]
pub(crate) struct PythonStreamTypes<'ir> {
    partial_classes: Vec<PartialPythonClass<'ir>>,
}

/// The Python class corresponding to Partial<TypeDefinedInBaml>
struct PartialPythonClass<'ir> {
    name: &'ir str,
    fields: Vec<(&'ir str, String)>,
}

impl<'ir> TryFrom<&'ir IntermediateRepr> for PythonTypes<'ir> {
    type Error = anyhow::Error;

    fn try_from(ir: &'ir IntermediateRepr) -> Result<PythonTypes<'ir>> {
        Ok(PythonTypes {
            enums: ir.walk_enums().map(PythonEnum::from).collect::<Vec<_>>(),
            classes: ir.walk_classes().map(PythonClass::from).collect::<Vec<_>>(),
        })
    }
}

impl<'ir> From<EnumWalker<'ir>> for PythonEnum<'ir> {
    fn from(e: EnumWalker<'ir>) -> PythonEnum<'ir> {
        PythonEnum {
            name: e.name(),
            values: e
                .item
                .elem
                .values
                .iter()
                .map(|v| v.elem.0.as_str())
                .collect(),
        }
    }
}

impl<'ir> From<ClassWalker<'ir>> for PythonClass<'ir> {
    fn from(c: ClassWalker<'ir>) -> Self {
        PythonClass {
            name: c.name(),
            fields: c
                .item
                .elem
                .static_fields
                .iter()
                .map(|f| (f.elem.name.as_str(), f.elem.r#type.elem.to_type_ref()))
                .collect(),
        }
    }
}

impl<'ir> TryFrom<&'ir IntermediateRepr> for PythonStreamTypes<'ir> {
    type Error = anyhow::Error;

    fn try_from(ir: &'ir IntermediateRepr) -> Result<Self> {
        Ok(Self {
            partial_classes: ir
                .walk_classes()
                .map(PartialPythonClass::from)
                .collect::<Vec<_>>(),
        })
    }
}

impl<'ir> From<ClassWalker<'ir>> for PartialPythonClass<'ir> {
    fn from(c: ClassWalker<'ir>) -> PartialPythonClass<'ir> {
        PartialPythonClass {
            name: c.name(),
            fields: c
                .item
                .elem
                .static_fields
                .iter()
                .map(|f| {
                    (
                        f.elem.name.as_str(),
                        f.elem.r#type.elem.to_partial_type_ref(),
                    )
                })
                .collect(),
        }
    }
}

trait ToTypeReferenceInTypeDefinition {
    fn to_type_ref(&self) -> String;
    fn to_partial_type_ref(&self) -> String;
}

impl ToTypeReferenceInTypeDefinition for FieldType {
    fn to_type_ref(&self) -> String {
        match self {
            FieldType::Class(name) | FieldType::Enum(name) => format!("\"{name}\""),
            FieldType::List(inner) => format!("List[{}]", inner.to_type_ref()),
            FieldType::Map(key, value) => {
                format!("Dict[{}, {}]", key.to_type_ref(), value.to_type_ref())
            }
            FieldType::Primitive(r#type) => r#type.to_python(),
            FieldType::Union(inner) => format!(
                "Union[{}]",
                inner
                    .iter()
                    .map(|t| t.to_type_ref())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Tuple(inner) => format!(
                "Tuple[{}]",
                inner
                    .iter()
                    .map(|t| t.to_type_ref())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Optional(inner) => format!("Optional[{}]", inner.to_type_ref()),
        }
    }

    fn to_partial_type_ref(&self) -> String {
        match self {
            FieldType::Class(name) => format!("\"{name}\""),
            FieldType::Enum(name) => format!("Optional[types.{name}]"),
            FieldType::List(inner) => format!("List[{}]", inner.to_partial_type_ref()),
            FieldType::Map(key, value) => {
                format!(
                    "Dict[{}, {}]",
                    key.to_partial_type_ref(),
                    value.to_partial_type_ref()
                )
            }
            FieldType::Primitive(r#type) => format!("Optional[{}]", r#type.to_python()),
            FieldType::Union(inner) => format!(
                "Optional[Union[{}]]",
                inner
                    .iter()
                    .map(|t| t.to_partial_type_ref())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Tuple(inner) => format!(
                "Optional[Tuple[{}]]",
                inner
                    .iter()
                    .map(|t| t.to_partial_type_ref())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Optional(inner) => inner.to_partial_type_ref(),
        }
    }
}
