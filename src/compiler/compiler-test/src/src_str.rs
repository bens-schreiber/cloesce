/// Cloesce source string containing a wide variety of features for codegen tests.
pub const COMPREHENSIVE_SRC: &str = r#"
// Top level comment
d1 {
    db
}

kv MyKv {
    someValue(id1: string, id2: int) -> json {
        "value/{id1}/{id2}"
    }

    streamValue(id1: string, id2: int) -> stream {
        "stream/{id1}/{id2}"
    }
}

r2 MyR2 {
    fileData(id: string) {
        "files/{id}"
    }

    metadata(ownerId: string, modelYear: int) {
        "meta/{ownerId}/{modelYear}"
    }

    photoData(modelYear: int) {
        "photos/{modelYear}"
    }

    customDsData(id: int) {
        "custom/{id}/data"
    }
}

durable LeaderboardDo {
    shard {
        [gt 0]
        tenantId: int
    }

    topEntryCache() -> json {
        "top"
    }

    topEntryCacheWithDate(date: string) -> json {
        "top/{date}"
    }
}

durable GlobalDo {
    config() -> json {
        "config"
    }
}

vars {
    // Comment again
    MY_VAR: string // More comments
}

inject { YouTubeApi }

model BasicModel for db {
    primary {
        id: int
    }

    foreign OneToManyModel::id {
        fk_to_model
    }
}

model HasSqlColumnTypes for db {
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

model HasOneToOne for db {
    primary {
        id: int
    }

    foreign BasicModel::id {
        basicModelId
    }

    nav BasicModel::id(basicModelId) { oneToOneNav }
}

model OneToManyModel for db {
    primary {
        id: int
    }

    nav BasicModel::fk_to_model {
        oneToManyNav
    }
}

model ModelWithCompositePk for db {
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

model ModelWithKv for db {
    primary {
        id1: string
        id2: int
    }

    kv MyKv::someValue(id1, id2) {
        someValue
    }

    kv MyKv::streamValue(id1, id2) {
        streamValue
    }
}

api ModelWithKv {
    [inject db]
    post instanceMethod(self, input: string) -> string
    get staticMethod(input: int) -> int
    post hasKvParamAndRes(self, input: kvobject<string>) -> kvobject<string>
}

model ModelWithR2 for db {
    primary {
        id: string
    }

    r2 MyR2::fileData(id) {
        fileData
    }
}

api ModelWithR2 {
    post hasR2ParamAndRes(self, input: r2object) -> r2object
}

model ToyotaPrius for db {
    primary {
        id: int
    }

    column {
        ownerId: string
        modelYear: int
    }

    kv MyKv::someValue(ownerId, modelYear) {
        metadata
    }

    r2 MyR2::photoData(modelYear) {
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

[crud get, save, list]
model ModelWithCruds for db {
    primary {
        id: int
    }

    column {
        name: string
    }

    foreign BasicModel::id {
        categoryId
    }
}

source ByName for ModelWithCruds {
    include {}

    get([instance] name: string)

    list(name: string, limit: int)
}

model ModelWithCustomDs for db {
    primary {
        id: int
    }

    column {
        name: string
    }

    r2 MyR2::customDsData(id) {
        data
    }

    foreign OneToManyModel::id {
        oneToManyId
    }

    nav OneToManyModel::id(oneToManyId) { oneToManyModel }
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

[crud get, save]
model RouteOwner {
    route {
        ownerId: string
        modelYear: int
    }

    kv MyKv::someValue(ownerId, modelYear) {
        metadata
    }

    nav RouteCar::ownerId(ownerId) { car }
}

model RouteCar {
    route {
        ownerId: string
    }
}

api RouteOwner {
    post instanceMethod(self, input: string) -> string
}

model BasicService {}
api BasicService {
    get downloadData() -> stream
    post instanceMethod(input: int) -> int
    get staticMethod(input: string) -> string
    post uploadData(data: stream) -> bool

    [inject LeaderboardDo(tenantId)]
    get topScores(tenantId: int) -> json

    [inject GlobalDo()]
    get globalConfig() -> json
}

[crud get, save]
model Leaderboard for LeaderboardDo(tenantId) {
    kv LeaderboardDo::topEntryCache {
        topEntries
    }
}

model GlobalSettings for GlobalDo {
    kv GlobalDo::config {
        config
    }
}

[crud get, list, save]
model LeaderboardEntry for LeaderboardDo(tenantId) {
    primary {
        id: int
    }

    column {
        playerName: string
        score: int
    }

    kv LeaderboardDo::topEntryCache {
        topEntries
    }
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
