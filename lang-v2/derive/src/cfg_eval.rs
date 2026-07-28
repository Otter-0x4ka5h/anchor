use {
    std::env,
    syn::{
        punctuated::Punctuated, Attribute, Expr, ExprLit, Fields, Lit, Meta, MetaList,
        MetaNameValue, Token,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CfgMatch {
    Enabled,
    Disabled,
    Unknown,
}

impl CfgMatch {
    fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Disabled, _) | (_, Self::Disabled) => Self::Disabled,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Enabled, Self::Enabled) => Self::Enabled,
        }
    }

    fn or(self, other: Self) -> Self {
        match (self, other) {
            (Self::Enabled, _) | (_, Self::Enabled) => Self::Enabled,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Disabled, Self::Disabled) => Self::Disabled,
        }
    }

    fn not(self) -> Self {
        match self {
            Self::Enabled => Self::Disabled,
            Self::Disabled => Self::Enabled,
            Self::Unknown => Self::Unknown,
        }
    }
}

pub(crate) fn cfg_attrs_match(attrs: &[Attribute]) -> syn::Result<bool> {
    let mut first_unknown = None;

    for attr in attrs.iter().filter(|attr| attr.path().is_ident("cfg")) {
        match cfg_attr_matches(attr)? {
            CfgMatch::Enabled => {}
            CfgMatch::Disabled => return Ok(false),
            CfgMatch::Unknown => {
                first_unknown.get_or_insert(attr);
            }
        }
    }

    if let Some(attr) = first_unknown {
        Err(unsupported_cfg_error(attr))
    } else {
        Ok(true)
    }
}

pub(crate) fn cfg_attrs(attrs: &[Attribute]) -> Vec<Attribute> {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("cfg"))
        .cloned()
        .collect()
}

pub(crate) fn cfg_attrs_match_if_known(attrs: &[Attribute]) -> syn::Result<Option<bool>> {
    let mut saw_unknown = false;

    for attr in attrs.iter().filter(|attr| attr.path().is_ident("cfg")) {
        match cfg_attr_matches(attr)? {
            CfgMatch::Enabled => {}
            CfgMatch::Disabled => return Ok(Some(false)),
            CfgMatch::Unknown => saw_unknown = true,
        }
    }

    Ok((!saw_unknown).then_some(true))
}

pub(crate) fn filter_fields(fields: &Fields) -> syn::Result<Fields> {
    Ok(match fields {
        Fields::Named(named) => {
            let mut filtered = named.clone();
            filtered.named =
                named
                    .named
                    .iter()
                    .try_fold(Punctuated::new(), |mut filtered_fields, field| {
                        if cfg_attrs_match(&field.attrs)? {
                            filtered_fields.push(field.clone());
                        }
                        Ok::<Punctuated<syn::Field, syn::token::Comma>, syn::Error>(filtered_fields)
                    })?;
            Fields::Named(filtered)
        }
        Fields::Unnamed(unnamed) => {
            let mut filtered = unnamed.clone();
            filtered.unnamed = unnamed.unnamed.iter().try_fold(
                Punctuated::new(),
                |mut filtered_fields, field| {
                    if cfg_attrs_match(&field.attrs)? {
                        filtered_fields.push(field.clone());
                    }
                    Ok::<Punctuated<syn::Field, syn::token::Comma>, syn::Error>(filtered_fields)
                },
            )?;
            Fields::Unnamed(filtered)
        }
        Fields::Unit => Fields::Unit,
    })
}

pub(crate) fn filter_variants(
    variants: &Punctuated<syn::Variant, syn::token::Comma>,
) -> syn::Result<Punctuated<syn::Variant, syn::token::Comma>> {
    variants
        .iter()
        .try_fold(Punctuated::new(), |mut filtered_variants, variant| {
            if cfg_attrs_match(&variant.attrs)? {
                filtered_variants.push(variant.clone());
            }
            Ok::<Punctuated<syn::Variant, syn::token::Comma>, syn::Error>(filtered_variants)
        })
}

fn cfg_attr_matches(attr: &Attribute) -> syn::Result<CfgMatch> {
    attr.parse_args::<Meta>()
        .map(|meta| cfg_meta_matches(&meta))
}

fn cfg_meta_matches(meta: &Meta) -> CfgMatch {
    match meta {
        Meta::Path(path) => cfg_flag_matches(&path_tail(path)),
        Meta::NameValue(nv) => cfg_name_value_matches(nv),
        Meta::List(list) => cfg_meta_list_matches(list),
    }
}

fn cfg_meta_list_matches(list: &MetaList) -> CfgMatch {
    let name = path_tail(&list.path);
    let nested = list
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .unwrap_or_default();
    match name.as_str() {
        "all" => nested.iter().fold(CfgMatch::Enabled, |state, meta| {
            state.and(cfg_meta_matches(meta))
        }),
        "any" => nested.iter().fold(CfgMatch::Disabled, |state, meta| {
            state.or(cfg_meta_matches(meta))
        }),
        "not" => nested
            .first()
            .map(|meta| cfg_meta_matches(meta).not())
            .unwrap_or(CfgMatch::Disabled),
        _ => CfgMatch::Unknown,
    }
}

fn cfg_name_value_matches(nv: &MetaNameValue) -> CfgMatch {
    let name = path_tail(&nv.path);
    let Some(value) = literal_value(&nv.value) else {
        return CfgMatch::Unknown;
    };

    if name == "feature" {
        return if env::var_os(feature_env_name(&value)).is_some() {
            CfgMatch::Enabled
        } else {
            CfgMatch::Disabled
        };
    }

    env::var(cfg_env_name(&name))
        .map(|actual| actual.split(',').any(|candidate| candidate == value))
        .map(|matches| {
            if matches {
                CfgMatch::Enabled
            } else {
                CfgMatch::Disabled
            }
        })
        .unwrap_or(CfgMatch::Unknown)
}

fn cfg_flag_matches(name: &str) -> CfgMatch {
    if env::var_os(cfg_env_name(name)).is_some() {
        CfgMatch::Enabled
    } else {
        CfgMatch::Unknown
    }
}

fn unsupported_cfg_error(attr: &Attribute) -> syn::Error {
    syn::Error::new_spanned(
        attr,
        "Anchor cannot safely evaluate this cfg during macro expansion; use a Cargo feature here or move the cfg outside the macro-generated item",
    )
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
        assert!(cfg_attrs_match(&attrs).unwrap());

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

        let filtered = filter_fields(&item.fields).unwrap();
        let Fields::Named(named) = filtered else {
            panic!("expected named fields");
        };
        assert_eq!(named.named.len(), 1);
        assert_eq!(named.named[0].ident.as_ref().unwrap().to_string(), "always");
    }

    #[test]
    fn cfg_attrs_match_rejects_unknown_target_predicates() {
        let _guard = ENV_LOCK.lock().unwrap();
        let target_os_key = cfg_env_name("target_os");
        env::remove_var(&target_os_key);

        let direct: Vec<Attribute> = vec![syn::parse_quote!(#[cfg(target_os = "solana")])];
        assert_eq!(cfg_attrs_match_if_known(&direct).unwrap(), None);
        assert!(cfg_attrs_match(&direct).is_err());

        let negated: Vec<Attribute> = vec![syn::parse_quote!(#[cfg(not(target_os = "solana"))])];
        assert_eq!(cfg_attrs_match_if_known(&negated).unwrap(), None);
        assert!(cfg_attrs_match(&negated).is_err());
    }
}
