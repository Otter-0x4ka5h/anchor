use anyhow::{anyhow, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use syn::parse::{Error as ParseError, Result as ParseResult};
use syn::spanned::Spanned;
use syn::{Ident, ImplItem, ImplItemConst, Type, TypePath};

/// Crate parse context
///
/// Keeps track of modules defined within a crate.
pub struct CrateContext {
    modules: BTreeMap<String, ParsedModule>,
}

impl CrateContext {
    pub fn parse(root: impl AsRef<Path>) -> Result<Self> {
        Ok(CrateContext {
            modules: ParsedModule::parse_recursive(root.as_ref())?,
        })
    }

    pub fn consts(&self) -> impl Iterator<Item = &syn::ItemConst> {
        self.modules.iter().flat_map(|(_, ctx)| ctx.consts())
    }

    pub fn impl_consts(&self) -> impl Iterator<Item = (&Ident, &syn::ImplItemConst)> {
        self.modules.iter().flat_map(|(_, ctx)| ctx.impl_consts())
    }

    pub fn structs(&self) -> impl Iterator<Item = &syn::ItemStruct> {
        self.modules.iter().flat_map(|(_, ctx)| ctx.structs())
    }

    pub fn enums(&self) -> impl Iterator<Item = &syn::ItemEnum> {
        self.modules.iter().flat_map(|(_, ctx)| ctx.enums())
    }

    pub fn type_aliases(&self) -> impl Iterator<Item = &syn::ItemType> {
        self.modules.iter().flat_map(|(_, ctx)| ctx.type_aliases())
    }

    pub fn modules(&self) -> impl Iterator<Item = ModuleContext<'_>> {
        self.modules.values().map(|detail| ModuleContext { detail })
    }

    pub fn root_module(&self) -> ModuleContext<'_> {
        ModuleContext {
            detail: self.modules.get("crate").unwrap(),
        }
    }

    // Perform Anchor safety checks on the parsed create
    pub fn safety_checks(&self) -> Result<()> {
        // Check all structs for unsafe field types, i.e. AccountInfo and UncheckedAccount.
        for ctx in self.modules.values() {
            for unsafe_field in ctx.unsafe_struct_fields() {
                // Check if unsafe field type has been documented with a /// SAFETY: doc string.
                let is_documented = unsafe_field.attrs.iter().any(|attr| {
                    attr.tokens.clone().into_iter().any(|token| match token {
                        // Check for doc comments containing CHECK
                        proc_macro2::TokenTree::Literal(s) => s.to_string().contains("CHECK"),
                        _ => false,
                    })
                });
                if !is_documented {
                    let ident = unsafe_field.ident.as_ref().unwrap();
                    let span = ident.span();
                    // Error if undocumented.
                    return Err(anyhow!(
                        r#"
        {}:{}:{}
        Struct field "{}" is unsafe, but is not documented.
        Please add a `/// CHECK:` doc comment explaining why no checks through types are necessary.
        Alternatively, for reasons like quick prototyping, you may disable the safety checks
        by using the `skip-lint` option.
        See https://www.anchor-lang.com/docs/the-accounts-struct#safety-checks for more information.
                    "#,
                        ctx.file.canonicalize().unwrap().display(),
                        span.start().line,
                        span.start().column,
                        ident.to_string()
                    ));
                };
            }
        }

        // Check for missing account reloads after CPI calls
        for ctx in self.modules.values() {
            self.check_missing_account_reloads_simple(ctx)?;
        }

        Ok(())
    }

    /// looking for common patterns
    fn check_missing_account_reloads_simple(&self, module: &ParsedModule) -> Result<()> {
        for item in &module.items {
            if let syn::Item::Mod(mod_item) = item {
                if mod_item
                    .attrs
                    .iter()
                    .any(|attr| attr.path.is_ident("program"))
                {
                    if let Some((_, items)) = &mod_item.content {
                        for item in items {
                            match item {
                                syn::Item::Fn(fn_item) => {
                                    // Check functions for missing account reloads
                                    self.check_function_for_missing_reloads_simple(
                                        fn_item,
                                        &module.file,
                                    )?;
                                }
                                syn::Item::Impl(impl_item) => {
                                    for impl_item in &impl_item.items {
                                        if let syn::ImplItem::Method(method) = impl_item {
                                            self.check_method_for_missing_reloads_simple(
                                                method,
                                                &module.file,
                                            )?;
                                        }
                                    }
                                }
                                _ => {} // Ignore other items
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// reload after CPI calls means invoke/invoke_signed
    fn check_function_for_missing_reloads_simple(
        &self,
        function: &syn::ItemFn,
        file: &Path,
    ) -> Result<()> {
        let function_str = quote::quote! { #function }.to_string();

        // invoke(...) patterns
        if function_str.contains("invoke") {
            let lines: Vec<&str> = function_str.lines().collect();
            let mut found_invoke = false;
            let mut found_account_access_after_invoke = false;
            let mut found_reload = false;
            let mut account_variables = Vec::new();

            for line in lines.iter() {
                let line = line.trim();

                if line.contains("invoke") {
                    found_invoke = true;
                    found_reload = line.contains(".reload()");
                    continue;
                }

                if line.contains("ctx.accounts") && line.contains("let") && line.contains("=") {
                    if let Some(var_name) = self.extract_account_variable_name(line) {
                        account_variables.push(var_name);
                    }
                }

                if found_invoke {
                    if line.contains(".reload()") {
                        found_reload = true;
                        continue;
                    }

                    for var_name in &account_variables {
                        if line.contains(&format!("{}.", var_name)) && !found_reload {
                            // Found account field access without reload after invoke
                            found_account_access_after_invoke = true;
                            break;
                        }
                    }

                    // Also check for direct ctx.accounts access
                    if line.contains("ctx.accounts") && !line.contains(".reload()") && !found_reload
                    {
                        found_account_access_after_invoke = true;
                        break;
                    }
                }
            }

            // If we found account access after invoke without reload, report error
            if found_account_access_after_invoke {
                return Err(anyhow!(
                    r#"
        {}:{}:{}
        Missing account reload!
        
        Account data is accessed after a CPI call (invoke/invoke_signed) without being reloaded.
        CPI calls can modify account data, so you must call `ctx.accounts.<account_name>.reload()?` 
        after the CPI call and before accessing the account data.
        See https://www.anchor-lang.com/docs/account-types#reloading-accounts for more information.
                    "#,
                    file.canonicalize().unwrap().display(),
                    function.sig.span().start().line,
                    function.sig.span().start().column
                ));
            }
        }

        Ok(())
    }

    fn check_method_for_missing_reloads_simple(
        &self,
        method: &syn::ImplItemMethod,
        file: &Path,
    ) -> Result<()> {
        let method_str = quote::quote! { #method }.to_string();

        if method_str.contains("invoke") {
            let lines: Vec<&str> = method_str.lines().collect();
            let mut found_invoke = false;
            let mut found_account_access_after_invoke = false;
            let mut found_reload = false;
            let mut account_variables = Vec::new();

            for line in &lines {
                let line = line.trim();

                if line.contains("invoke") {
                    found_invoke = true;
                    found_reload = false;
                    continue;
                }

                if line.contains("ctx.accounts") && line.contains("let") && line.contains("=") {
                    if let Some(var_name) = self.extract_account_variable_name(line) {
                        account_variables.push(var_name);
                    }
                }

                if found_invoke {
                    if line.contains(".reload()") {
                        found_reload = true;
                        continue;
                    }

                    for var_name in &account_variables {
                        if line.contains(&format!("{}.", var_name)) && !found_reload {
                            found_account_access_after_invoke = true;
                            break;
                        }
                    }

                    // Also check for direct ctx.accounts access
                    if line.contains("ctx.accounts") && !line.contains(".reload()") && !found_reload
                    {
                        found_account_access_after_invoke = true;
                        break;
                    }
                }
            }

            // If we found account access after invoke without reload, report error
            if found_account_access_after_invoke {
                return Err(anyhow!(
                    r#"
        {}:{}:{}
        Missing account reload!
        
        Account data is accessed after a CPI call (invoke/invoke_signed) without being reloaded.
        CPI calls can modify account data, so you must call `ctx.accounts.<account_name>.reload()?` 
        after the CPI call and before accessing the account data.
        See https://www.anchor-lang.com/docs/account-types#reloading-accounts for more information.
                    "#,
                    file.canonicalize().unwrap().display(),
                    method.sig.span().start().line,
                    method.sig.span().start().column
                ));
            }
        }

        Ok(())
    }

    /// Extract account variable name from assignment line
    /// e.g., "let user_account = &mut ctx.accounts.user_account;" -> "user_account"
    fn extract_account_variable_name(&self, line: &str) -> Option<String> {
        if let Some(start) = line.find("let ") {
            if let Some(end) = line[start + 4..].find(" =") {
                let var_name = line[start + 4..start + 4 + end].trim();
                if !var_name.is_empty() {
                    return Some(var_name.to_string());
                }
            }
        }
        None
    }
}

/// Module parse context
///
/// Keeps track of items defined within a module.
#[derive(Copy, Clone)]
pub struct ModuleContext<'krate> {
    detail: &'krate ParsedModule,
}

impl ModuleContext<'_> {
    pub fn items(&self) -> impl Iterator<Item = &syn::Item> {
        self.detail.items.iter()
    }
}
struct ParsedModule {
    name: String,
    file: PathBuf,
    path: String,
    items: Vec<syn::Item>,
}

struct UnparsedModule {
    file: PathBuf,
    path: String,
    name: String,
    item: syn::ItemMod,
}

impl ParsedModule {
    fn parse_recursive(root: &Path) -> Result<BTreeMap<String, ParsedModule>> {
        let mut modules = BTreeMap::new();

        let root_content = std::fs::read_to_string(root)?;
        let root_file = syn::parse_file(&root_content)?;
        let root_mod = Self::new(
            String::new(),
            root.to_owned(),
            "crate".to_owned(),
            root_file.items,
        );

        let mut unparsed = root_mod.unparsed_submodules();
        while let Some(to_parse) = unparsed.pop() {
            let path = format!("{}::{}", to_parse.path, to_parse.name);
            let module = Self::from_item_mod(&to_parse.file, &path, to_parse.item)?;

            unparsed.extend(module.unparsed_submodules());
            modules.insert(format!("{}{}", module.path, to_parse.name), module);
        }

        modules.insert(root_mod.name.clone(), root_mod);

        Ok(modules)
    }

    fn from_item_mod(
        parent_file: &Path,
        parent_path: &str,
        item: syn::ItemMod,
    ) -> ParseResult<Self> {
        Ok(match item.content {
            Some((_, items)) => {
                // The module content is within the parent file being parsed
                Self::new(
                    parent_path.to_owned(),
                    parent_file.to_owned(),
                    item.ident.to_string(),
                    items,
                )
            }
            None => {
                // The module is referencing some other file, so we need to load that
                // to parse the items it has.
                let parent_dir = parent_file.parent().unwrap();
                let parent_filename = parent_file.file_stem().unwrap().to_str().unwrap();
                let parent_mod_dir = parent_dir.join(parent_filename);

                let possible_file_paths = vec![
                    parent_dir.join(format!("{}.rs", item.ident)),
                    parent_dir.join(format!("{}/mod.rs", item.ident)),
                    parent_mod_dir.join(format!("{}.rs", item.ident)),
                    parent_mod_dir.join(format!("{}/mod.rs", item.ident)),
                ];

                let mod_file_path = possible_file_paths
                    .into_iter()
                    .find(|p| p.exists())
                    .ok_or_else(|| ParseError::new_spanned(&item, "could not find file"))?;
                let mod_file_content = std::fs::read_to_string(&mod_file_path)
                    .map_err(|_| ParseError::new_spanned(&item, "could not read file"))?;
                let mod_file = syn::parse_file(&mod_file_content)?;

                Self::new(
                    parent_path.to_owned(),
                    mod_file_path,
                    item.ident.to_string(),
                    mod_file.items,
                )
            }
        })
    }

    fn new(path: String, file: PathBuf, name: String, items: Vec<syn::Item>) -> Self {
        Self {
            name,
            file,
            path,
            items,
        }
    }

    fn unparsed_submodules(&self) -> Vec<UnparsedModule> {
        self.submodules()
            .map(|item| UnparsedModule {
                file: self.file.clone(),
                path: self.path.clone(),
                name: item.ident.to_string(),
                item: item.clone(),
            })
            .collect()
    }

    fn submodules(&self) -> impl Iterator<Item = &syn::ItemMod> {
        self.items.iter().filter_map(|i| match i {
            syn::Item::Mod(item) => Some(item),
            _ => None,
        })
    }

    fn structs(&self) -> impl Iterator<Item = &syn::ItemStruct> {
        self.items.iter().filter_map(|i| match i {
            syn::Item::Struct(item) => Some(item),
            _ => None,
        })
    }

    fn unsafe_struct_fields(&self) -> impl Iterator<Item = &syn::Field> {
        let accounts_filter = |item_struct: &&syn::ItemStruct| {
            item_struct.attrs.iter().any(|attr| {
                match attr.parse_meta() {
                    Ok(syn::Meta::List(syn::MetaList{path, nested, ..})) => {
                        path.is_ident("derive") && nested.iter().any(|nested| {
                            matches!(nested, syn::NestedMeta::Meta(syn::Meta::Path(path)) if path.is_ident("Accounts"))
                        })
                    }
                    _ => false
                }
            })
        };

        self.structs()
            .filter(accounts_filter)
            .flat_map(|s| &s.fields)
            .filter(|f| match &f.ty {
                syn::Type::Path(syn::TypePath {
                    path: syn::Path { segments, .. },
                    ..
                }) => {
                    segments.len() == 1 && segments[0].ident == "UncheckedAccount"
                        || segments[0].ident == "AccountInfo"
                }
                _ => false,
            })
    }

    fn enums(&self) -> impl Iterator<Item = &syn::ItemEnum> {
        self.items.iter().filter_map(|i| match i {
            syn::Item::Enum(item) => Some(item),
            _ => None,
        })
    }

    fn type_aliases(&self) -> impl Iterator<Item = &syn::ItemType> {
        self.items.iter().filter_map(|i| match i {
            syn::Item::Type(item) => Some(item),
            _ => None,
        })
    }

    fn consts(&self) -> impl Iterator<Item = &syn::ItemConst> {
        self.items.iter().filter_map(|i| match i {
            syn::Item::Const(item) => Some(item),
            _ => None,
        })
    }

    fn impl_consts(&self) -> impl Iterator<Item = (&Ident, &ImplItemConst)> {
        self.items
            .iter()
            .filter_map(|i| match i {
                syn::Item::Impl(syn::ItemImpl {
                    self_ty: ty, items, ..
                }) => {
                    if let Type::Path(TypePath {
                        qself: None,
                        path: p,
                    }) = ty.as_ref()
                    {
                        if let Some(ident) = p.get_ident() {
                            let mut to_return = Vec::new();
                            items.iter().for_each(|item| {
                                if let ImplItem::Const(item) = item {
                                    to_return.push((ident, item));
                                }
                            });
                            Some(to_return)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .flatten()
    }
}
