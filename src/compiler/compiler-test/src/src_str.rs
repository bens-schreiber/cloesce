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

    column {
        str: string
        integer: int
        dub: real
        boo: bool
        dat: date
        strNull: option<string>
        integerNull: option<int>
        dubNull: option<real>
        booNull: option<bool>
        dateNull: option<date>
    }
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

    column {
        name: string
    }
}

api ModelWithCompositePk {
    [inject db]
    post instanceMethod(self, input: string) -> string
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
    [inject db]
    post instanceMethod(self, input: string) -> string
    get staticMethod(input: int) -> int
    post hasKvParamAndRes(self, input: kvobject<string>) -> kvobject<string>
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
    post hasR2ParamAndRes(self, input: r2object) -> r2object
}

[use db]
model ToyotaPrius {
    primary {
        id: int
    }

    column {
        ownerId: string
        modelYear: int
    }

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

[use db]
[crud get, save, list]
model ModelWithCruds {
    primary {
        id: int
    }

    column {
        name: string
    }

    foreign(BasicModel::id) {
        categoryId
    }
}

source ByName for ModelWithCruds {
    include {}

    get([instance] name: string)

    list(name: string, limit: int)
}

[use db]
model ModelWithCustomDs {
    primary {
        id: int
    }

    column {
        name: string
    }

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

    get([instance] id: int, externalParam: string)
}

api ModelWithCustomDs {
    post instanceMethod([source Custom] self, input: string) -> string
}

model BasicService {}
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
