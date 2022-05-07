use crate::errors::{Error, Kind, Result};
use crate::scope::Env;
use crate::tag::{Tag, Tagged};
use oal_syntax::ast::{
    Array, AsExpr, Expr, NodeRef, Object, Operator, Relation, Uri, UriSegment, VariadicOp,
};

trait TypeChecked {
    fn type_check(&self) -> Result<()> {
        Ok(())
    }
}

impl<T: AsExpr + Tagged> TypeChecked for VariadicOp<T> {
    fn type_check(&self) -> Result<()> {
        match self.op {
            Operator::Join => {
                if self.exprs.iter().all(|e| e.unwrap_tag() == Tag::Object) {
                    Ok(())
                } else {
                    Err(Error::new(Kind::InvalidTypes, "ill-formed join").with(self))
                }
            }
            Operator::Any | Operator::Sum => {
                if self.exprs.iter().all(|e| e.unwrap_tag().is_schema()) {
                    Ok(())
                } else {
                    Err(Error::new(Kind::InvalidTypes, "ill-formed alternative").with(self))
                }
            }
        }
    }
}

impl<T: AsExpr + Tagged> TypeChecked for Relation<T> {
    fn type_check(&self) -> Result<()> {
        let uri_check = self.uri.unwrap_tag() == Tag::Uri;
        let xfers_check = self.xfers.values().all(|t| {
            if let Some(t) = t {
                let domain_check = if let Some(d) = &t.domain {
                    d.unwrap_tag() == Tag::Content
                } else {
                    true
                };
                let range_check = t.range.unwrap_tag() == Tag::Content;
                domain_check && range_check
            } else {
                true
            }
        });
        if uri_check && xfers_check {
            Ok(())
        } else {
            Err(Error::new(Kind::InvalidTypes, "ill-formed relation").with(self))
        }
    }
}

impl<T: AsExpr + Tagged> TypeChecked for Uri<T> {
    fn type_check(&self) -> Result<()> {
        let vars_check = self.spec.iter().all(|s| {
            if let UriSegment::Variable(v) = s {
                v.val.unwrap_tag() == Tag::Primitive
            } else {
                true
            }
        });
        if vars_check {
            Ok(())
        } else {
            Err(Error::new(Kind::InvalidTypes, "ill-formed URI").with(self))
        }
    }
}

impl<T: AsExpr + Tagged> TypeChecked for Array<T> {
    fn type_check(&self) -> Result<()> {
        if self.item.unwrap_tag().is_schema() {
            Ok(())
        } else {
            Err(Error::new(Kind::InvalidTypes, "ill-formed array").with(self))
        }
    }
}

impl<T: AsExpr + Tagged> TypeChecked for Object<T> {
    fn type_check(&self) -> Result<()> {
        if self.props.iter().all(|p| p.val.unwrap_tag().is_schema()) {
            Ok(())
        } else {
            Err(Error::new(Kind::InvalidTypes, "ill-formed object").with(self))
        }
    }
}

pub fn type_check<T>(_acc: &mut (), _env: &mut Env<T>, node: NodeRef<T>) -> Result<()>
where
    T: AsExpr + Tagged,
{
    if let NodeRef::Expr(e) = node {
        match e.as_ref() {
            Expr::Op(op) => op.type_check(),
            Expr::Rel(rel) => rel.type_check(),
            Expr::Uri(uri) => uri.type_check(),
            Expr::Array(arr) => arr.type_check(),
            Expr::Object(obj) => obj.type_check(),
            _ => Ok(()),
        }
    } else {
        Ok(())
    }
}
