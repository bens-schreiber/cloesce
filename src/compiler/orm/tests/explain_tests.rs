//! Snapshot tests for the query-plan explainer. Planning only — no MockStorage / sqlx.

mod common;

use common::setup::tree;
use compiler_test::src_to_idl;
use orm::query::explain::{explain_save, explain_select};
use orm::query::save::planner::plan as save_plan;
use orm::query::select::planner::{SelectOperation, plan as select_plan};
use serde_json::{Value, json};

/// A complex schema with several different backings and relationships
/// used to exercise the planner and explainer.
const SRC: &str = r#"
    d1 { db }

    durable BoardDo {
        shard { tenantId: int }

        topCache -> json {
            "top"
        }
    }

    r2 Bucket {
        banner {
            pid: int
            "banners/{pid}"
        }
    }

    model Org for db {
        primary { id: int }
        column { tenantId: int }
        one Board::tenantId(tenantId) { board }
    }

    model Board for BoardDo(tenantId) {
        primary { pid: int }
        r2 Bucket::banner(pid) { banner }
        kv BoardDo::{ topCache(), tenantId(tenantId) } { top }
        many Entry::{ tenantId(tenantId), boardId(pid) } { entries }
    }

    model Entry for BoardDo(tenantId) {
        primary { id: int }
        column { score: int }
        foreign Board::pid { boardId }
    }
"#;

fn include() -> Value {
    json!({
        "board": {
            "banner": {},
            "top": {},
            "entries": {},
        }
    })
}

#[test]
fn explain_save_snapshot() {
    let idl = src_to_idl(SRC);
    let payload = json!({
        "tenantId": 7,
        "board": {
            "tenantId": 7,
            "banner": { "url": "b.png" },
            "top": { "cached": true },
            "entries": [ { "tenantId": 7, "score": 42 } ]
        }
    });

    let plan = save_plan("Org", &idl, &tree(include()), &payload).expect("plan");
    insta::assert_snapshot!(explain_save("Org", &tree(include()), &plan));
}

#[test]
fn explain_select_list_snapshot() {
    let idl = src_to_idl(SRC);
    let inc = tree(include());
    let plan = select_plan(SelectOperation::List, "Org", &idl, &inc);
    insta::assert_snapshot!(explain_select(SelectOperation::List, "Org", &inc, &plan));
}

#[test]
fn explain_select_get_snapshot() {
    let idl = src_to_idl(SRC);
    let inc = tree(include());
    let plan = select_plan(SelectOperation::Get, "Org", &idl, &inc);
    insta::assert_snapshot!(explain_select(SelectOperation::Get, "Org", &inc, &plan));
}
