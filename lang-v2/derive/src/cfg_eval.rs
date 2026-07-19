use {
    std::env,
    syn::{
        punctuated::Punctuated, Attribute, Expr, ExprLit, Fields, Lit, Meta, MetaList,
        MetaNameValue, Token,
    },
};

pub(crate) fn cfg_attrs_match(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("cfg"))
        .all(cfg_attr_matches)
}

pub(crate) fn filter_fields(fields: &Fields) -> Fields {
    match fields {
        Fields::Named(named) => {
            let mut filtered = named.clone();
            filtered.named = named
                .named
                .iter()
                .filter(|field| cfg_attrs_match(&field.attrs))
                .cloned()
                .collect();
            Fields::Named(filtered)
        }
        Fields::Unnamed(unnamed) => {
            let mut filtered = unnamed.clone();
            filtered.unnamed = unnamed
                .unnamed
                .iter()
                .filter(|field| cfg_attrs_match(&field.attrs))
                .cloned()
                .collect();
            Fields::Unnamed(filtered)
        }
        Fields::Unit => Fields::Unit,
    }
}

pub(crate) fn filter_variants(
    variants: &Punctuated<syn::Variant, syn::token::Comma>,
) -> Punctuated<syn::Variant, syn::token::Comma> {
    variants
        .iter()
        .filter(|variant| cfg_attrs_match(&variant.attrs))
        .cloned()
        .collect()
}

fn cfg_attr_matches(attr: &Attribute) -> bool {
    attr.parse_args::<Meta>()
        .map(|meta| cfg_meta_matches(&meta))
        .unwrap_or(true)
}

fn cfg_meta_matches(meta: &Meta) -> bool {
    match meta {
        Meta::Path(path) => env::var_os(cfg_env_name(&path_tail(path))).is_some(),
        Meta::NameValue(nv) => cfg_name_value_matches(nv),
        Meta::List(list) => cfg_meta_list_matches(list),
    }
}

fn cfg_meta_list_matches(list: &MetaList) -> bool {
    let name = path_tail(&list.path);
    let nested = list
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .unwrap_or_default();
    match name.as_str() {
        "all" => nested.iter().all(cfg_meta_matches),
        "any" => nested.iter().any(cfg_meta_matches),
        "not" => nested
            .first()
            .map(|meta| !cfg_meta_matches(meta))
            .unwrap_or(false),
        _ => false,
    }
}

fn cfg_name_value_matches(nv: &MetaNameValue) -> bool {
    let name = path_tail(&nv.path);
    let Some(value) = literal_value(&nv.value) else {
        return false;
    };

    if name == "feature" {
        return env::var_os(feature_env_name(&value)).is_some();
    }

    env::var(cfg_env_name(&name))
        .map(|actual| actual.split(',').any(|candidate| candidate == value))
        .unwrap_or(false)
}

fn literal_value(expr: &Expr) -> Option<String> {
    let Expr::Lit(ExprLit { lit, .. }) = expr else {
        return None;
    };
    match lit {
        Lit::Str(s) => Some(s.value()),
        Lit::Int(i) => Some(i.base10_digits().to_owned()),
        Lit::Bool(b) => Some(b.value.to_string()),
        _ => None,
    }
}

fn cfg_env_name(name: &str) -> String {
    format!("CARGO_CFG_{}", env_key_fragment(name))
}

fn feature_env_name(feature: &str) -> String {
    format!("CARGO_FEATURE_{}", env_key_fragment(feature))
}

fn env_key_fragment(value: &str) -> String {
    value.replace('-', "_").to_ascii_uppercase()
}

fn path_tail(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn cfg_attrs_match_supports_feature_predicates() {
        let _guard = ENV_LOCK.lock().unwrap();
        let live_key = feature_env_name("live");
        let hidden_key = feature_env_name("hidden");

        env::set_var(&live_key, "1");
        env::remove_var(&hidden_key);

        let attrs: Vec<Attribute> = vec![syn::parse_quote!(
            #[cfg(all(feature = "live", not(feature = "hidden")))]
        )];
        assert!(cfg_attrs_match(&attrs));

        env::remove_var(&live_key);
    }

    #[test]
    fn filter_fields_omits_cfg_disabled_members() {
        let _guard = ENV_LOCK.lock().unwrap();
        let disabled_key = feature_env_name("disabled");
        env::remove_var(&disabled_key);

        let item: syn::ItemStruct = syn::parse_quote! {
            struct Demo {
                pub always: u64,
                #[cfg(feature = "disabled")]
                pub hidden: u64,
            }
        };

        let filtered = filter_fields(&item.fields);
        let Fields::Named(named) = filtered else {
            panic!("expected named fields");
        };
        assert_eq!(named.named.len(), 1);
        assert_eq!(named.named[0].ident.as_ref().unwrap().to_string(), "always");
    }
}
