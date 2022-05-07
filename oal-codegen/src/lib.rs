use indexmap::indexmap;
use oal_compiler::eval;
use oal_syntax::ast;
use openapiv3::{
    ArrayType, Info, MediaType, ObjectType, OpenAPI, Operation, Parameter, ParameterData,
    ParameterSchemaOrContent, PathItem, Paths, ReferenceOr, RequestBody, Response, Responses,
    Schema, SchemaData, SchemaKind, StringType, Type, VariantOrUnknownOrEmpty,
};

pub struct Builder {
    spec: eval::Spec,
}

impl Builder {
    pub fn new(s: eval::Spec) -> Builder {
        Builder { spec: s }
    }

    pub fn open_api(&self) -> OpenAPI {
        OpenAPI {
            openapi: "3.0.1".into(),
            info: Info {
                title: "Test OpenAPI specification".into(),
                version: "0.1.0".into(),
                ..Default::default()
            },
            paths: self.all_paths(),
            ..Default::default()
        }
    }

    fn media_type(&self) -> String {
        "application/json".into()
    }

    fn uri_pattern(&self, uri: &eval::Uri) -> String {
        uri.pattern()
    }

    fn prim_type(&self, prim: &ast::Primitive) -> Type {
        match prim {
            ast::Primitive::Num => Type::Number(Default::default()),
            ast::Primitive::Str => Type::String(Default::default()),
            ast::Primitive::Bool => Type::Boolean {},
        }
    }

    fn prim_schema(&self, prim: &ast::Primitive) -> Schema {
        Schema {
            schema_data: Default::default(),
            schema_kind: SchemaKind::Type(self.prim_type(prim)),
        }
    }

    fn rel_schema(&self, rel: &eval::Relation) -> Schema {
        self.uri_schema(&rel.uri)
    }

    fn uri_schema(&self, uri: &eval::Uri) -> Schema {
        let pattern = if uri.spec.is_empty() {
            None
        } else {
            Some(self.uri_pattern(uri).into())
        };
        Schema {
            schema_data: SchemaData {
                example: pattern,
                ..Default::default()
            },
            schema_kind: SchemaKind::Type(Type::String(StringType {
                format: VariantOrUnknownOrEmpty::Unknown("uri-reference".into()),
                ..Default::default()
            })),
        }
    }

    fn join_schema(&self, schemas: &Vec<eval::Schema>) -> Schema {
        Schema {
            schema_data: Default::default(),
            schema_kind: SchemaKind::AllOf {
                all_of: schemas
                    .iter()
                    .map(|s| ReferenceOr::Item(self.schema(s)))
                    .collect(),
            },
        }
    }

    fn object_type(&self, obj: &eval::Object) -> Type {
        Type::Object(ObjectType {
            properties: obj
                .props
                .iter()
                .map(|p| {
                    let ident = p.name.as_ref().into();
                    let expr = ReferenceOr::Item(self.schema(&p.schema).into());
                    (ident, expr)
                })
                .collect(),
            ..Default::default()
        })
    }

    fn object_schema(&self, obj: &eval::Object) -> Schema {
        Schema {
            schema_data: Default::default(),
            schema_kind: SchemaKind::Type(self.object_type(obj)),
        }
    }

    fn array_schema(&self, array: &eval::Array) -> Schema {
        Schema {
            schema_data: Default::default(),
            schema_kind: SchemaKind::Type(Type::Array(ArrayType {
                items: Some(ReferenceOr::Item(self.schema(array.item.as_ref()).into())),
                min_items: None,
                max_items: None,
                unique_items: false,
            })),
        }
    }

    fn sum_schema(&self, schemas: &Vec<eval::Schema>) -> Schema {
        Schema {
            schema_data: Default::default(),
            schema_kind: SchemaKind::OneOf {
                one_of: schemas
                    .iter()
                    .map(|s| ReferenceOr::Item(self.schema(s)))
                    .collect(),
            },
        }
    }

    fn any_schema(&self, schemas: &Vec<eval::Schema>) -> Schema {
        Schema {
            schema_data: Default::default(),
            schema_kind: SchemaKind::AnyOf {
                any_of: schemas
                    .iter()
                    .map(|s| ReferenceOr::Item(self.schema(s)))
                    .collect(),
            },
        }
    }

    fn schema(&self, s: &eval::Schema) -> Schema {
        let mut sch = match &s.expr {
            eval::Expr::Prim(prim) => self.prim_schema(prim),
            eval::Expr::Rel(rel) => self.rel_schema(rel),
            eval::Expr::Uri(uri) => self.uri_schema(uri),
            eval::Expr::Object(obj) => self.object_schema(obj),
            eval::Expr::Array(array) => self.array_schema(array),
            eval::Expr::Op(operation) => match operation.op {
                ast::Operator::Join => self.join_schema(&operation.schemas),
                ast::Operator::Sum => self.sum_schema(&operation.schemas),
                ast::Operator::Any => self.any_schema(&operation.schemas),
            },
        };
        sch.schema_data.description = s.desc.clone();
        sch
    }

    fn prop_param(&self, prop: &eval::Prop) -> Parameter {
        Parameter::Path {
            parameter_data: ParameterData {
                name: prop.name.as_ref().into(),
                description: None,
                required: true,
                deprecated: None,
                format: ParameterSchemaOrContent::Schema(ReferenceOr::Item(
                    self.schema(&prop.schema),
                )),
                example: None,
                examples: Default::default(),
                explode: None,
                extensions: Default::default(),
            },
            style: Default::default(),
        }
    }

    fn uri_params(&self, uri: &eval::Uri) -> Vec<Parameter> {
        uri.spec
            .iter()
            .flat_map(|s| match s {
                eval::UriSegment::Variable(p) => Some(self.prop_param(p)),
                _ => None,
            })
            .collect()
    }

    fn relation_path_item(&self, rel: &eval::Relation) -> PathItem {
        let parameters = self
            .uri_params(&rel.uri)
            .into_iter()
            .map(ReferenceOr::Item)
            .collect();

        let mut path_item = PathItem {
            parameters,
            ..Default::default()
        };

        let xfers = rel
            .xfers
            .iter()
            .filter_map(|(m, x)| x.as_ref().map(|x| (m, x)));

        for (method, xfer) in xfers {
            let request = xfer.domain.schema.as_ref().map(|schema| {
                ReferenceOr::Item(RequestBody {
                    content: indexmap! { self.media_type() => MediaType {
                        schema: Some(ReferenceOr::Item(self.schema(schema))),
                        ..Default::default()
                    }},
                    description: schema.desc.clone(),
                    ..Default::default()
                })
            });
            let response = xfer.range.schema.as_ref().map(|schema| {
                ReferenceOr::Item(Response {
                    content: indexmap! { self.media_type() => MediaType {
                        schema: Some(ReferenceOr::Item(self.schema(schema))),
                        ..Default::default()
                    }},
                    description: schema.desc.clone().unwrap_or("".to_owned()),
                    ..Default::default()
                })
            });
            let op = Operation {
                request_body: request,
                responses: Responses {
                    default: response,
                    ..Default::default()
                },
                ..Default::default()
            };

            match method {
                ast::Method::Get => path_item.get = Some(op),
                ast::Method::Put => path_item.put = Some(op),
                ast::Method::Post => path_item.post = Some(op),
                ast::Method::Patch => path_item.patch = Some(op),
                ast::Method::Delete => path_item.delete = Some(op),
                ast::Method::Options => path_item.options = Some(op),
                ast::Method::Head => path_item.head = Some(op),
            }
        }

        path_item
    }

    fn all_paths(&self) -> Paths {
        Paths {
            paths: self
                .spec
                .rels
                .iter()
                .map(|(pattern, rel)| {
                    (
                        pattern.clone(),
                        ReferenceOr::Item(self.relation_path_item(rel)),
                    )
                })
                .collect(),
            extensions: Default::default(),
        }
    }
}
