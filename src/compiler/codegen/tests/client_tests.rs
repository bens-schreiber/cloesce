use compiler_test::{SemanticResult, src_to_ast};

#[test]
fn test_client_code_generation_snapshot() {
    let src = r#"
        env {
            db: d1
            my_kv: kv
            my_r2: r2
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

        // @d1(db)
        // model ManyToManyModelA {
        //     [primary id]
        //     id: int

        //     [nav manyToManyNav <> ManyToManyModelB::manyToManyNav]
        //     manyToManyNav: Array<ManyToManyModelB>
        // }

        // @d1(db)
        // model ManyToManyModelB {
        //     [primary id]
        //     id: int

        //     [nav manyToManyNav <> ManyToManyModelA::manyToManyNav]
        //     manyToManyNav: Array<ManyToManyModelA>
        // }

        // @d1(db)
        // model ModelWithCompositePk {
        //     [primary tenantId, rowId]
        //     tenantId: string
        //     rowId: int

        //     name: string
        // }

        // api ModelWithCompositePkApi for ModelWithCompositePk {
        //     post instanceMethod(input: string) -> string
        // }

        // model ModelWithKv {
        //     @keyparam
        //     id1: string

        //     @keyparam
        //     id2: string

        //     @kv(my_kv, "{id1}")
        //     someValue: json

        //     @kv(my_kv, "")
        //     manyValues: Paginated<json>

        //     @kv(my_kv, "constant")
        //     streamValue: stream
        // }

        // api ModelWithKvApi for ModelWithKv {
        //     post instanceMethod(input: string) -> string
        //     get staticMethod(input: int) -> int
        //     post hasKvParamAndRes(input: KvObject<string>) -> KvObject<string>
        // }

        // model ModelWithR2 {
        //     @keyparam
        //     id: string

        //     @r2(my_r2,"{id}")
        //     fileData: blob

        //     @r2(my_r2, "{id}/files")
        //     manyFileDatas: Paginated<blob>
        // }

        // api ModelWithR2Api for ModelWithR2 {
        //     post instanceMethod(input: string) -> string
        //     get staticMethod(input: int) -> int
        //     post hasR2ParamAndRes(input: R2Object) -> R2Object
        // }

        // @d1(db)
        // model ToyotaPrius {
        //     [primary id]
        //     id: int

        //     ownerId: string
        //     modelYear: int

        //     @keyparam
        //     ownerId: string

        //     @keyparam
        //     modelYear: int

        //     @kv(my_kv, "{ownerId}/{modelYear}")
        //     metadata: json

        //     @r2(my_r2, "{modelYear}/photos")
        //     photoData: blob
        // }

        // source WithKv for ToyotaPrius {
        //     include {
        //         metadata
        //     }
        // }

        // source WithR2 for ToyotaPrius {
        //     include {
        //         photoData
        //     }
        // }

        // @d1(db)
        // @crud(get, save, list)
        // model ModelWithCruds {
        //     [primary id]
        //     id: int

        //     name: string
        // }

        // service BasicService {}
        // api BasicServiceApi for BasicService {
        //     post instanceMethod(input: string) -> string
        //     get staticMethod(input: int) -> int
        //     post hasStreamParam(input: stream) -> string
        //     get hasStreamRes() -> stream
        // }

        // poo BasicPoo {
        //     field1: string
        //     field2: int
        // }

        // poo PooWithComposition {
        //     field1: BasicPoo
        //     field2: BasicModel
        // }
    
    "#;
    let SemanticResult { ast, .. } = src_to_ast(src);

    // let client_code = ClientGenerator::generate(&ast, "http://example.com/path/to/api");
}
