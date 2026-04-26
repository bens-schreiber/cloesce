/// Cloesce source string containing a wide variety of features for codegen tests.
pub const COMPREHENSIVE_SRC: &str = r#"
// Top level comment
env {
// Comments
    d1 { db }
    // Can exist anywhere in code
    kv { my_kv }
    r2 { my_r2 } // Another Comment
    vars {
        // Comment again
        MY_VAR: string // More comments
    }
}

inject { YouTubeApi } 

[use db]
model BasicModel {
    primary {
        id: int
    }

    foreign(OneToManyModel::id) {
        fk_to_model
    }
}

[use db]
model HasSqlColumnTypes {
    primary {
        id: int
    }

    str: string
    integer: int
    dub: real
    boo: bool
    dat: date
    strNull: Option<string>
    integerNull: Option<int>
    dubNull: Option<real>
    booNull: Option<bool>
    dateNull: Option<date>
}

[use db]
model HasOneToOne {
    primary {
        id: int
    }

    foreign(BasicModel::id) {
        basicModelId
        nav { oneToOneNav }
    }
}

[use db]
model OneToManyModel {
    primary {
        id: int
    }

    nav(BasicModel::fk_to_model) {
        oneToManyNav
    }
}

[use db]
model ManyToManyModelA {
    primary {
        id: int
    }

    nav(ManyToManyModelB::id) {
        manyToManyNav
    }
}

[use db]
model ManyToManyModelB {
    primary {
        id: int
    }

    nav(ManyToManyModelA::id) {
        manyToManyNav
    }
}

[use db]
model ModelWithCompositePk {
    primary {
        tenantId: string
        rowId: int
    }

    name: string
}

api ModelWithCompositePk {
    post instanceMethod(self, e: env, input: string) -> string
}

model ModelWithKv {
    keyfield {
        id1: string
        id2: int
    }

    kv(my_kv, "{id1}") {
        someValue: json
    }

    kv(my_kv, "") paginated {
        manyValues: json
    }

    kv(my_kv, "{id1}/{id2}") {
        streamValue: stream
    }
}

api ModelWithKv {
    post instanceMethod(self, e: env, input: string) -> string
    get staticMethod(input: int) -> int
    post hasKvParamAndRes(self, input: KvObject<string>) -> KvObject<string>
}

model ModelWithR2 {
    keyfield {
        id: string
    }

    r2(my_r2, "{id}") {
        fileData
    }

    r2(my_r2, "{id}/files") paginated {
        manyFileDatas
    }
}

api ModelWithR2 {
    post hasR2ParamAndRes(self, input: R2Object) -> R2Object
}

[use db]
model ToyotaPrius {
    primary {
        id: int
    }

    ownerId: string
    modelYear: int

    keyfield {
        someKey: string
    }

    kv(my_kv, "{ownerId}/{modelYear}") {
        metadata: json
    }

    r2(my_r2, "{modelYear}/photos") {
        photoData
    }
}

api ToyotaPrius {
    post instanceMethod(self, input: string) -> string
}

source WithKv for ToyotaPrius {
    include {
        metadata
    }
}

source WithR2 for ToyotaPrius {
    include {
        photoData
    }
}

[use db, get, save, list]
model ModelWithCruds {
    primary {
        id: int
    }

    name: string

    foreign(BasicModel::id) {
        categoryId
    }
}

source ByName for ModelWithCruds {
    include {}

    sql get(name: string) {
        "SELECT * FROM ModelWithCruds WHERE name = $name"
    }

    sql list(name: string, limit: int) {
        "SELECT * FROM ModelWithCruds WHERE name LIKE $name LIMIT $limit"
    }
}

[use db]
model ModelWithCustomDs {
    primary {
        id: int
    }

    name: string

    r2 (my_r2, "{id}/data") {
        data
    }

    foreign (OneToManyModel::id) {
        oneToManyId
        nav { oneToManyModel }
    }
}

source Custom for ModelWithCustomDs {
    include {
        oneToManyModel {
            oneToManyNav
        }
        data
    }

    sql get(id: int, externalParam: string) {
        "SELECT * FROM ModelWithCustomDs WHERE id = $id AND name LIKE $externalParam"
    }
}

api ModelWithCustomDs {
    post instanceMethod([source Custom] self, input: string) -> string
}

service BasicService {}
api BasicService {
    get downloadData() -> stream
    post instanceMethod(input: int) -> int
    get staticMethod(input: string) -> string
    post uploadData(data: stream) -> bool
}

poo BasicPoo {
    field1: string
    field2: int
}

poo PooWithComposition {
    field1: BasicPoo
    field2: BasicModel
}
    "#;
