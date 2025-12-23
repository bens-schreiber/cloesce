mod mappers;

use std::sync::Arc;

use ast::{
    CidlType, CloesceAst, HttpVerb, InputLanguage, MediaType, NavigationProperty,
    NavigationPropertyKind, cidl_type_contains,
};
use mappers::{ClientLanguageTypeMapper, TypeScriptMapper};

use handlebars::{Handlebars, handlebars_helper};

handlebars_helper!(needs_constructor: |cidl_type: CidlType| matches!(cidl_type.root_type(),
    CidlType::Object(_)
    | CidlType::Blob
    | CidlType::Partial(_)
    | CidlType::DateIso
    | CidlType::Stream
));

handlebars_helper!(get_object_name: |cidl_type: CidlType| match cidl_type.root_type() {
    CidlType::Inject(name) | CidlType::Object(name) | CidlType::Partial(name) => serde_json::to_value(name).unwrap(),
    ty => serde_json::to_value(ty).unwrap()
});
handlebars_helper!(get_content_type: |media: MediaType| match media {
    MediaType::Json=>"application/json",
    MediaType::Octet => "application/octet-stream",
});

handlebars_helper!(is_blob: |cidl_type: CidlType| matches!(cidl_type.root_type(), CidlType::Blob));
handlebars_helper!(is_one_to_one: |nav: NavigationProperty| matches!(nav.kind, NavigationPropertyKind::OneToOne {..}));
handlebars_helper!(is_many_nav: |nav: NavigationProperty| matches!(nav.kind, NavigationPropertyKind::OneToMany {..} | NavigationPropertyKind::ManyToMany { .. }));
handlebars_helper!(is_get_request: |verb: HttpVerb| matches!(verb, HttpVerb::GET));
handlebars_helper!(is_serializable: |cidl_type: CidlType| !matches!(cidl_type.root_type(), CidlType::Inject(_)));
handlebars_helper!(is_object: |cidl_type: CidlType| matches!(cidl_type.root_type(), CidlType::Object(_) | CidlType::Partial(_)));

// TODO: This method of generating fromJson for arrays won't help for n-dimensional arrays
handlebars_helper!(has_array: |cidl_type: CidlType| cidl_type_contains!(&cidl_type, CidlType::Array(_)));
handlebars_helper!(is_object_array: |cidl_type: CidlType| matches!(cidl_type.root_type(), CidlType::Object(_)) && cidl_type_contains!(&cidl_type, CidlType::Array(_)));
handlebars_helper!(is_blob_array: |cidl_type: CidlType| matches!(cidl_type.root_type(), CidlType::Blob) && cidl_type_contains!(&cidl_type, CidlType::Array(_)));

// If a parameter should be placed in the url instead of the body.
// True for any [CidlType::DataSource] or given the verb [HttpVerb::GET]
handlebars_helper!(is_url_param: |cidl_type: CidlType, verb: HttpVerb| matches!(verb, HttpVerb::GET) || matches!(cidl_type, CidlType::DataSource(_)));

const TYPESCRIPT_TEMPLATE: &str = include_str!("./templates/ts.hbs");
const TEMPLATE_STRING: &str = "client_api";

pub struct ClientGenerator;
impl ClientGenerator {
    pub fn generate_client_api(ast: &CloesceAst, domain: String) -> String {
        let template = match ast.language {
            InputLanguage::TypeScript => TYPESCRIPT_TEMPLATE,
            // InputLanguage::...
        };

        let mapper = match ast.language {
            InputLanguage::TypeScript => Arc::new(TypeScriptMapper),
            // InputLanguage::...
        };

        let mut handlebars = Handlebars::new();
        handlebars
            .register_template_string(TEMPLATE_STRING, template)
            .unwrap();
        register_helpers(&mut handlebars, mapper, ast);

        let mut context = serde_json::to_value(ast).unwrap();

        // Manually get the domain in there
        if let serde_json::Value::Object(ref mut map) = context {
            map.insert("domain".to_string(), serde_json::Value::String(domain));
        }

        handlebars.render(TEMPLATE_STRING, &context).unwrap()
    }
}

fn register_helpers<'a>(
    handlebars: &mut Handlebars<'a>,
    mapper: Arc<dyn ClientLanguageTypeMapper + Send + Sync>,
    ast: &'a CloesceAst,
) {
    handlebars.register_helper("is_serializable", Box::new(is_serializable));
    handlebars.register_helper("is_blob", Box::new(is_blob));
    handlebars.register_helper("is_one_to_one", Box::new(is_one_to_one));
    handlebars.register_helper("is_many_nav", Box::new(is_many_nav));
    handlebars.register_helper("get_content_type", Box::new(get_content_type));
    handlebars.register_helper("has_array", Box::new(has_array));
    handlebars.register_helper("needs_constructor", Box::new(needs_constructor));
    handlebars.register_helper("get_object_name", Box::new(get_object_name));
    handlebars.register_helper("is_object", Box::new(is_object));
    handlebars.register_helper("is_object_array", Box::new(is_object_array));
    handlebars.register_helper("is_blob_array", Box::new(is_blob_array));
    handlebars.register_helper("is_url_param", Box::new(is_url_param));
    handlebars.register_helper("is_get_request", Box::new(is_get_request));

    let mapper1 = mapper.clone();
    handlebars.register_helper(
        "get_nav_cidl_type",
        Box::new(
            move |h: &handlebars::Helper<'_>,
                  _: &Handlebars,
                  _: &handlebars::Context,
                  _: &mut handlebars::RenderContext<'_, '_>,
                  out: &mut dyn handlebars::Output| {
                let nav: NavigationProperty =
                    serde_json::from_value(h.param(0).unwrap().value().clone()).unwrap();

                let cidl_type = match nav.kind {
                    NavigationPropertyKind::OneToOne { .. } => CidlType::Object(nav.model_name),
                    NavigationPropertyKind::OneToMany { .. }
                    | NavigationPropertyKind::ManyToMany { .. } => {
                        CidlType::array(CidlType::Object(nav.model_name))
                    }
                };

                let rendered = mapper1.cidl_type(&cidl_type, ast);
                out.write(&rendered)?;
                Ok(())
            },
        ),
    );

    let mapper2 = mapper.clone();
    handlebars.register_helper(
        "map_cidl_type",
        Box::new(
            move |h: &handlebars::Helper<'_>,
                  _: &Handlebars,
                  _: &handlebars::Context,
                  _: &mut handlebars::RenderContext<'_, '_>,
                  out: &mut dyn handlebars::Output| {
                let cidl_type: CidlType =
                    serde_json::from_value(h.param(0).unwrap().value().clone()).unwrap();

                let rendered = mapper2.cidl_type(&cidl_type, ast);
                out.write(&rendered)?;
                Ok(())
            },
        ),
    );

    let mapper3 = mapper.clone();
    handlebars.register_helper(
        "map_root_cidl_type",
        Box::new(
            move |h: &handlebars::Helper<'_>,
                  _: &Handlebars,
                  _: &handlebars::Context,
                  _: &mut handlebars::RenderContext<'_, '_>,
                  out: &mut dyn handlebars::Output| {
                let cidl_type: CidlType =
                    serde_json::from_value(h.param(0).unwrap().value().clone()).unwrap();

                let rendered = mapper3.cidl_type(cidl_type.root_type(), ast);
                out.write(&rendered)?;
                Ok(())
            },
        ),
    );

    let mapper4 = mapper.clone();
    handlebars.register_helper(
        "get_media_type",
        Box::new(
            move |h: &handlebars::Helper<'_>,
                  _: &Handlebars,
                  _: &handlebars::Context,
                  _: &mut handlebars::RenderContext<'_, '_>,
                  out: &mut dyn handlebars::Output| {
                let media_type: MediaType =
                    serde_json::from_value(h.param(0).unwrap().value().clone()).unwrap();

                let rendered = mapper4.media_type(&media_type);
                out.write(&rendered)?;
                Ok(())
            },
        ),
    );
}
