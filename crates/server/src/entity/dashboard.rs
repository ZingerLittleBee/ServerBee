use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize, utoipa::ToSchema)]
#[sea_orm(table_name = "dashboard")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::dashboard_widget::Entity")]
    Widgets,
}

impl Related<super::dashboard_widget::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Widgets.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
