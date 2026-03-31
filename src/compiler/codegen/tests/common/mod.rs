/// Cloesce source string containing a wide variety of features for codegen tests.
pub const COMPREHENSIVE_SRC: &str = r#"
env {
    db: d1
    my_kv: kv
    my_r2: r2
    MY_VAR: string
}

@d1(db)
model BasicModel {
    [primary id]
    id: int

    [foreign fk_to_model -> OneToManyModel::id]
    fk_to_model: int
}

@d1(db)
model HasSqlColumnTypes {
    [primary id]
    id: int

    str: string
    integer: int
    dub: double
    boo: bool
    dat: date
    strNull: Option<string>
    integerNull: Option<int>
    dubNull: Option<double>
    booNull: Option<bool>
    dateNull: Option<date>
}

@d1(db)
model HasOneToOne {
    [primary id]
    id: int

    [foreign basicModelId -> BasicModel::id]
    basicModelId: int

    [nav oneToOneNav -> basicModelId]
    oneToOneNav: BasicModel
}

@d1(db)
model OneToManyModel {
    [primary id]
    id: int

    [nav oneToManyNav -> BasicModel::fk_to_model]
    oneToManyNav: Array<BasicModel>
}

@d1(db)
model ManyToManyModelA {
    [primary id]
    id: int

    [nav manyToManyNav <> ManyToManyModelB::manyToManyNav]
    manyToManyNav: Array<ManyToManyModelB>
}

@d1(db)
model ManyToManyModelB {
    [primary id]
    id: int

    [nav manyToManyNav <> ManyToManyModelA::manyToManyNav]
    manyToManyNav: Array<ManyToManyModelA>
}

@d1(db)
model ModelWithCompositePk {
    [primary tenantId, rowId]
    tenantId: string
    rowId: int

    name: string
}

api ModelWithCompositePk {
    post instanceMethod(self, e: env, input: string) -> string
}

model ModelWithKv {
    @keyparam
    id1: string

    @keyparam
    id2: string

    @kv(my_kv, "{id1}")
    someValue: json

    @kv(my_kv, "")
    manyValues: Paginated<json>

    @kv(my_kv, "constant")
    streamValue: stream
}

api ModelWithKv {
    post instanceMethod(self, e: env, input: string) -> string
    get staticMethod(input: int) -> int
    post hasKvParamAndRes(self, input: KvObject<string>) -> KvObject<string>
}

model ModelWithR2 {
    @keyparam
    id: string

    @r2(my_r2, "{id}")
    fileData: R2Object

    @r2(my_r2, "{id}/files")
    manyFileDatas: Paginated<R2Object>
}

api ModelWithR2 {
    post hasR2ParamAndRes(self, input: R2Object) -> R2Object
}

@d1(db)
model ToyotaPrius {
    [primary id]
    id: int

    ownerId: string
    modelYear: int

    @keyparam
    someKey: string

    @kv(my_kv, "{ownerId}/{modelYear}")
    metadata: json

    @r2(my_r2, "{modelYear}/photos")
    photoData: R2Object
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

@d1(db)
@crud(get, save, list)
model ModelWithCruds {
    [primary id]
    id: int

    name: string

    [foreign categoryId -> BasicModel::id]
    categoryId: int
}

source WithName for ModelWithCruds {
    include {}

    sql get(name: string) {
        "SELECT * FROM ModelWithCruds WHERE name = ?"
    }

    sql list(name: string, limit: int) {
        "SELECT * FROM ModelWithCruds WHERE name LIKE ? LIMIT ?"
    }
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
