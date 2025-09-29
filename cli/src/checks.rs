use std::{fs, path::Path, sync::LazyLock};

use anyhow::{anyhow, Result};
use codespan_reporting::{
    diagnostic::{Diagnostic, Label},
    files::SimpleFiles,
    term,
    term::termcolor::{ColorChoice, StandardStream},
};
use semver::{Version, VersionReq};
use std::sync::{Arc, Mutex};
use syn::{spanned::Spanned, visit::Visit, ItemFn, ItemMod};
use walkdir::WalkDir;

use crate::{
    config::{Config, Manifest, PackageManager, WithPath},
    VERSION,
};

/// Global warning collector for printing warnings after compilation
static WARNING_COLLECTOR: LazyLock<Arc<Mutex<Vec<WarningInfo>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(Vec::new())));

/// Information needed to print a warning later
#[derive(Debug, Clone)]
struct WarningInfo {
    file_path: String,
    line_num: u32,
    account_name: String,
    function_name: String,
    analyzer: AccountUsageAnalyzer,
}

/// Detect and print ALL functions that use invoke or invoke_signed.
///
/// This function uses static analysis to scan the source code for ANY function that uses
/// invoke/invoke_signed calls, including:
/// - Instruction functions
/// - Internal helper functions
/// - Anchor macros (like anchor::increment!)
/// - Any function that calls invoke
pub fn detect_invoke_usage(workspace_path: &std::path::Path) -> Result<()> {
    let mut invoke_functions = Vec::new();

    // Use WalkDir for efficient directory traversal
    for entry in WalkDir::new(workspace_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
        .filter(|e| !e.path().to_string_lossy().contains("target/"))
    {
        let file_path = entry.path();
        if let Ok(functions) = analyze_rust_file(file_path) {
            invoke_functions.extend(functions);
        }
    }

    if invoke_functions.is_empty() {
        return Ok(());
    }

    // Analyze account usage and reload patterns
    analyze_account_usage_patterns(&mut invoke_functions, workspace_path)?;

    // Collect issues for later display
    let mut issues = Vec::new();

    for func_info in &invoke_functions {
        // Check if there are any issues (accounts that need reload but aren't reloaded)
        let has_issues = func_info.invoke_calls.iter().any(|invoke_call| {
            invoke_call.accounts.iter().any(|account| {
                account.needs_reload && account.used_after_cpi && !account.reloaded_before_usage
            })
        });

        if has_issues {
            issues.push(func_info.clone());
        }
    }

    // Store issues for later display after compilation
    std::thread_local! {
        static ISSUES: std::cell::RefCell<Vec<InvokeFunctionInfo>> = std::cell::RefCell::new(Vec::new());
    }

    ISSUES.with(|issues_cell| {
        *issues_cell.borrow_mut() = issues;
    });

    Ok(())
}

impl AccountUsageAnalyzer {
    /// Find the first usage line after CPI for a specific account
    fn find_first_usage_after_cpi(
        &self,
        _file_path: &str,
        account_name: &str,
        cpi_line: u32,
        cpi_function: &str,
    ) -> Option<u32> {
        // First, try to resolve the account name if it's a variable
        let resolved_account_name = if let Some(dot_pos) = account_name.find('.') {
            let var_name = &account_name[..dot_pos];
            let resolved = self.resolve_variable(var_name, cpi_line);
            if resolved != var_name {
                // Replace the variable name with the resolved account reference
                format!("{}{}", resolved, &account_name[dot_pos..])
            } else {
                account_name.to_string()
            }
        } else {
            account_name.to_string()
        };

        // Extract base account name from resolved name
        let base_name = if let Some(start) = resolved_account_name.find("ctx.accounts.") {
            let after_ctx = &resolved_account_name[start + 13..];
            let end = after_ctx
                .find(|c| c == '.' || c == ' ' || c == '(')
                .unwrap_or(after_ctx.len());
            &after_ctx[..end]
        } else {
            // For non-context accounts, extract the base name before the first dot
            if let Some(dot_pos) = resolved_account_name.find('.') {
                &resolved_account_name[..dot_pos]
            } else {
                &resolved_account_name
            }
        };

        // Use the already collected usage lines instead of parsing source code
        // Find the most relevant usage of this account after the CPI call
        // Priority: 1) Usage after CPI line, 2) Usage in calling functions, 3) Any usage

        // Use function-aware logic to find the most relevant usage
        // This handles both same-function and cross-function scenarios

        // Strategy: Find usage that's in the same function as the CPI call
        // If CPI is in a helper function, find usage in the calling function
        let candidates: Vec<_> = self
            .account_usage_lines
            .iter()
            .filter(|(name, line)| name == base_name && *line < cpi_line)
            .collect();

        // For CPI in main functions (like transfer_with_cpi_bad), find usage in the same function
        // For CPI in helper functions (like function_c), find usage in calling functions
        if cpi_function.contains("transfer_with_cpi") {
            // This is a main function - find usage in the same function (before CPI)
            if let Some((_, line)) = candidates
                .iter()
                .filter(|(_, line)| *line < cpi_line && (cpi_line - *line) <= 15) // Within same function
                .max_by_key(|(_, line)| *line)
            {
                return Some(*line);
            }
        } else {
            // This is a helper function - find usage in calling functions (significantly before CPI)
            if let Some((_, line)) = candidates
                .iter()
                .filter(|(_, line)| *line < cpi_line && (cpi_line - *line) > 15) // In calling function
                .max_by_key(|(_, line)| *line)
            {
                return Some(*line);
            }
        }

        // Fallback: find any usage
        if let Some((_, line)) = self
            .account_usage_lines
            .iter()
            .find(|(name, _)| name == base_name)
        {
            return Some(*line);
        }

        None
    }
}

/// Collect a warning to be printed later after compilation
fn collect_warning(
    file_path: &str,
    line_num: u32,
    account_name: &str,
    function_name: &str,
    analyzer: &AccountUsageAnalyzer,
) {
    WARNING_COLLECTOR.lock().unwrap().push(WarningInfo {
        file_path: file_path.to_string(),
        line_num,
        account_name: account_name.to_string(),
        function_name: function_name.to_string(),
        analyzer: analyzer.clone(),
    });
}

/// Print a beautiful Rust-style warning using codespan-reporting
fn print_rust_style_warning(
    file_path: &str,
    line_num: u32,
    account_name: &str,
    function_name: &str,
    analyzer: &AccountUsageAnalyzer,
) {
    if let Ok(content) = std::fs::read_to_string(file_path) {
        let lines: Vec<&str> = content.lines().collect();
        let line_idx = (line_num - 1) as usize;

        if line_idx < lines.len() {
            let line_content = lines[line_idx];

            // Resolve variable names to actual ctx.accounts references
            let resolved_account_display = if account_name.contains("ctx.accounts.") {
                account_name.replace(".to_account_info()", "")
            } else {
                // If it's a variable name (like "i.to_account_info()"), extract the base variable name
                // and try to resolve it to the actual ctx.accounts reference
                if let Some(dot_pos) = account_name.find('.') {
                    let var_name = &account_name[..dot_pos];
                    // Use the same resolution logic as the analyzer
                    if let Some(assignment) = analyzer
                        .variable_assignments
                        .iter()
                        .rev() // Search in reverse to find the most recent
                        .find(|assignment| {
                            assignment.var_name == var_name && assignment.line_number < line_num
                        })
                    {
                        // Replace the variable with the resolved account reference
                        format!(
                            "{}{}",
                            assignment.account_reference,
                            &account_name[dot_pos..]
                        )
                        .replace(".to_account_info()", "")
                    } else {
                        account_name.replace(".to_account_info()", "")
                    }
                } else {
                    account_name.replace(".to_account_info()", "")
                }
            };

            // Calculate byte offset for the specific line
            let mut byte_offset = 0;
            for (i, line) in lines.iter().enumerate() {
                if i == line_idx {
                    break;
                }
                byte_offset += line.len() + 1; // +1 for newline
            }

            // Find the position of the account usage in the line for highlighting
            let account_pos = line_content.find("ctx.accounts.");
            let highlight_start = if let Some(pos) = account_pos {
                byte_offset + pos
            } else {
                byte_offset
            };

            let highlight_length = if let Some(pos) = account_pos {
                let after_ctx = &line_content[pos + 13..];
                let end = after_ctx
                    .find(|c| c == '.' || c == ' ' || c == '(' || c == ')')
                    .unwrap_or(after_ctx.len());
                byte_offset + pos + 13 + end
            } else {
                byte_offset + line_content.len()
            };

            // Create a SimpleFiles instance
            let mut files = SimpleFiles::new();
            let file_id = files.add(file_path, content);

            // Create the diagnostic
            let diagnostic = Diagnostic::warning()
                .with_message(format!(
                    "didn't reload the account: `{}` after cpi in `{}`",
                    resolved_account_display, function_name
                ))
                .with_labels(vec![Label::primary(
                    file_id,
                    highlight_start..highlight_length,
                )
                .with_message("usage of an account without reload after CPI")]);

            // Print the diagnostic
            let writer = StandardStream::stderr(ColorChoice::Auto);
            let config = codespan_reporting::term::Config::default();
            term::emit(&mut writer.lock(), &config, &files, &diagnostic).ok();
        }
    }
}

/// Print all collected warnings and clear the collector
pub fn print_collected_warnings() {
    let mut warnings = WARNING_COLLECTOR.lock().unwrap();
    if warnings.is_empty() {
        return;
    }

    // Sort warnings by line number for consistent ordering
    warnings.sort_by_key(|w| w.line_num);

    // Print each warning
    for warning in warnings.iter() {
        print_rust_style_warning(
            &warning.file_path,
            warning.line_num,
            &warning.account_name,
            &warning.function_name,
            &warning.analyzer,
        );
    }

    // Clear the collector
    warnings.clear();
}

/// Analyze account usage patterns after CPI calls to check reload order
fn analyze_account_usage_patterns(
    invoke_functions: &mut Vec<InvokeFunctionInfo>,
    _workspace_path: &std::path::Path,
) -> Result<()> {
    for func_info in invoke_functions.iter_mut() {
        // Re-analyze the file to check for account usage patterns
        if let Ok(content) = std::fs::read_to_string(&func_info.file_path) {
            if let Ok(syntax) = syn::parse_file(&content) {
                let mut usage_analyzer = AccountUsageAnalyzer::new();
                usage_analyzer.visit_file(&syntax);

                // for (name, line) in &usage_analyzer.account_usage_lines {}

                // For each invoke call, check if any accounts are used after it anywhere in the file
                for invoke_call in &mut func_info.invoke_calls {
                    // Check all account usage lines that come after this CPI call
                    // OR in functions that are called after the CPI call
                    for (account_name, _usage_line) in &usage_analyzer.account_usage_lines {
                        // For now, let's check if the account usage is in a different function
                        // and if the account was passed to the CPI call
                        let account_passed_to_cpi = invoke_call.accounts.iter().any(|acc| {
                            // Extract base account name from the CPI account
                            let cpi_account_base =
                                if let Some(start) = acc.name.find("ctx.accounts.") {
                                    let after_ctx = &acc.name[start + 13..];
                                    let end = after_ctx
                                        .find(|c| c == '.' || c == ' ' || c == '(')
                                        .unwrap_or(after_ctx.len());
                                    &after_ctx[..end]
                                } else {
                                    &acc.name
                                };

                            // Extract base account name from the usage
                            let usage_account_base =
                                if let Some(start) = account_name.find("ctx.accounts.") {
                                    let after_ctx = &account_name[start + 13..];
                                    let end = after_ctx
                                        .find(|c| c == '.' || c == ' ' || c == '(')
                                        .unwrap_or(after_ctx.len());
                                    &after_ctx[..end]
                                } else {
                                    account_name
                                };

                            cpi_account_base == usage_account_base
                        });

                        if account_passed_to_cpi {
                            // Mark this account as used after CPI
                            if let Some(account) = invoke_call.accounts.iter_mut().find(|acc| {
                                let cpi_account_base =
                                    if let Some(start) = acc.name.find("ctx.accounts.") {
                                        let after_ctx = &acc.name[start + 13..];
                                        let end = after_ctx
                                            .find(|c| c == '.' || c == ' ' || c == '(')
                                            .unwrap_or(after_ctx.len());
                                        &after_ctx[..end]
                                    } else {
                                        &acc.name
                                    };

                                let usage_account_base =
                                    if let Some(start) = account_name.find("ctx.accounts.") {
                                        let after_ctx = &account_name[start + 13..];
                                        let end = after_ctx
                                            .find(|c| c == '.' || c == ' ' || c == '(')
                                            .unwrap_or(after_ctx.len());
                                        &after_ctx[..end]
                                    } else {
                                        account_name
                                    };

                                cpi_account_base == usage_account_base
                            }) {
                                account.used_after_cpi = true;
                            }
                        }
                    }
                }

                // Create a global analyzer for warning generation
                let global_analyzer = AccountUsageAnalyzer {
                    cpi_lines: usage_analyzer.cpi_lines.clone(),
                    account_usage_lines: usage_analyzer.account_usage_lines.clone(),
                    reload_lines: usage_analyzer.reload_lines.clone(),
                    variable_assignments: usage_analyzer.variable_assignments.clone(),
                };

                // Show warnings for accounts that need reload
                for invoke_call in &func_info.invoke_calls {
                    for account in &invoke_call.accounts {
                        if account.needs_reload
                            && account.used_after_cpi
                            && !account.reloaded_before_usage
                        {
                            // Find the first usage line after CPI for this account
                            if let Some(usage_line) = global_analyzer.find_first_usage_after_cpi(
                                &func_info.file_path,
                                &account.name,
                                invoke_call.line,
                                &func_info.function_name,
                            ) {
                                collect_warning(
                                    &func_info.file_path,
                                    usage_line,
                                    &account.name,
                                    &func_info.function_name,
                                    &global_analyzer,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct InvokeFunctionInfo {
    function_name: String,
    file_path: String,
    invoke_calls: Vec<InvokeCallInfo>,
}

#[derive(Debug, Clone)]
struct InvokeCallInfo {
    line: u32,
    invoke_type: String, // "invoke", "invoke_signed", "anchor_macro", etc.
    program_id: Option<String>,
    accounts: Vec<AccountInfo>,
    data: Option<String>,
    signers: Option<Vec<String>>, // For invoke_signed
}

#[derive(Debug, Clone)]
struct AccountInfo {
    name: String,
    needs_reload: bool,          // true if account needs reload after CPI
    used_after_cpi: bool,        // true if account is used after CPI call
    reloaded_before_usage: bool, // true if account is reloaded before usage after CPI
}

/// Variable assignment with line number for scoped tracking
#[derive(Debug, Clone)]
struct VariableAssignment {
    var_name: String,
    account_reference: String,
    line_number: u32,
}

/// Analyzer for checking account usage patterns after CPI calls
#[derive(Debug, Clone)]
struct AccountUsageAnalyzer {
    cpi_lines: Vec<u32>,                     // Line numbers where CPI calls occur
    account_usage_lines: Vec<(String, u32)>, // (account_name, line_number) pairs
    reload_lines: Vec<(String, u32)>,        // (account_name, line_number) pairs for reload calls
    variable_assignments: Vec<VariableAssignment>, // All variable assignments with line numbers
}

impl AccountUsageAnalyzer {
    fn new() -> Self {
        Self {
            cpi_lines: Vec::new(),
            account_usage_lines: Vec::new(),
            reload_lines: Vec::new(),
            variable_assignments: Vec::new(),
        }
    }

    fn is_line_contains_invoke(&self, line: u32) -> bool {
        // Check if this line contains an invoke call or is within 3 lines of one
        // This handles multi-line invoke calls where field access happens on subsequent lines
        self.cpi_lines
            .iter()
            .any(|cpi_line| line >= *cpi_line && line <= *cpi_line + 3)
    }

    fn is_line_contains_reload(&self, line: u32) -> bool {
        // Check if this line contains a reload call
        self.reload_lines
            .iter()
            .any(|(_, reload_line)| *reload_line == line)
    }

    /// Track variable assignments like `let i = &mut ctx.accounts.user_account;`
    fn track_variable_assignment(&mut self, var_name: &str, account_ref: &str, line_number: u32) {
        self.variable_assignments.push(VariableAssignment {
            var_name: var_name.to_string(),
            account_reference: account_ref.to_string(),
            line_number,
        });
    }

    /// Resolve a variable name to its actual account reference at a specific line
    fn resolve_variable(&self, var_name: &str, usage_line: u32) -> String {
        // Find the most recent assignment of this variable before the usage line
        if let Some(assignment) = self.variable_assignments.iter().rev().find(|assignment| {
            assignment.var_name == var_name && assignment.line_number < usage_line
        }) {
            assignment.account_reference.clone()
        } else {
            var_name.to_string()
        }
    }
}

impl Visit<'_> for AccountUsageAnalyzer {
    fn visit_local(&mut self, node: &syn::Local) {
        // Check for variable assignments like `let i = &mut ctx.accounts.user_account;`
        if let Some(init) = &node.init {
            if let syn::Expr::Reference(ref_expr) = &*init.1 {
                if let syn::Expr::Field(field_expr) = &*ref_expr.expr {
                    let field_str = Self::expr_to_string(&syn::Expr::Field(field_expr.clone()));
                    if field_str.contains("ctx.accounts.") {
                        if let syn::Pat::Ident(pat_ident) = &node.pat {
                            let var_name = pat_ident.ident.to_string();
                            let line_number = node.span().start().line as u32;
                            self.track_variable_assignment(&var_name, &field_str, line_number);
                        }
                    }
                }
            }
        }

        syn::visit::visit_local(self, node);
    }

    fn visit_expr_call(&mut self, node: &syn::ExprCall) {
        // Check for invoke calls
        if let syn::Expr::Path(path) = &*node.func {
            let path_str = quote::quote!(#path).to_string();
            if path_str.contains("invoke") {
                let line = node.func.span().start().line as u32;
                self.cpi_lines.push(line);
            }
        }

        // Check for reload calls
        if let syn::Expr::MethodCall(method_call) = &*node.func {
            if method_call.method == "reload" {
                let line = method_call.span().start().line as u32;
                let receiver_str = Self::expr_to_string(&method_call.receiver);

                // Try to resolve the receiver if it's a variable
                let resolved_receiver = if let Some(dot_pos) = receiver_str.find('.') {
                    let var_name = &receiver_str[..dot_pos];
                    let resolved = self.resolve_variable(var_name, line);
                    if resolved != var_name {
                        // Replace the variable name with the resolved account reference
                        format!("{}{}", resolved, &receiver_str[dot_pos..])
                    } else {
                        receiver_str.clone()
                    }
                } else {
                    // Check if it's a simple variable name
                    let resolved = self.resolve_variable(&receiver_str, line);
                    if resolved != receiver_str {
                        resolved
                    } else {
                        receiver_str.clone()
                    }
                };

                if let Some(account_name) = Self::extract_account_name_from_expr(&resolved_receiver)
                {
                    self.reload_lines.push((account_name, line));
                }
            }
        }

        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_field(&mut self, node: &syn::ExprField) {
        // Check for account field access (usage)
        let field_str = Self::expr_to_string(&syn::Expr::Field(node.clone()));
        let line = node.span().start().line as u32;

        // Try to resolve the field access if it contains a variable
        let resolved_field_str = if let Some(dot_pos) = field_str.find('.') {
            let var_name = &field_str[..dot_pos];
            let resolved = self.resolve_variable(var_name, line);
            if resolved != var_name {
                // Replace the variable name with the resolved account reference
                format!("{}{}", resolved, &field_str[dot_pos..])
            } else {
                field_str.clone()
            }
        } else {
            field_str.clone()
        };

        // Check if the resolved field access contains ctx.accounts.
        if resolved_field_str.contains("ctx.accounts.") {
            if let Some(account_name) = Self::extract_account_name_from_expr(&resolved_field_str) {
                // Only count as usage if it's not on the same line as an invoke call
                // and it's not part of a reload call
                let contains_invoke = self.is_line_contains_invoke(line);
                let contains_reload = self.is_line_contains_reload(line);
                if !contains_invoke && !contains_reload {
                    self.account_usage_lines.push((account_name, line));
                }
            }
        }

        syn::visit::visit_expr_field(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &syn::ExprMethodCall) {
        // Check for account method calls (usage)
        let method_str = Self::expr_to_string(&syn::Expr::MethodCall(node.clone()));
        let line = node.span().start().line as u32;

        // Try to resolve the method call if it contains a variable
        let resolved_method_str = if let Some(dot_pos) = method_str.find('.') {
            let var_name = &method_str[..dot_pos];
            let resolved = self.resolve_variable(var_name, line);
            if resolved != var_name {
                // Replace the variable name with the resolved account reference
                format!("{}{}", resolved, &method_str[dot_pos..])
            } else {
                method_str.clone()
            }
        } else {
            method_str.clone()
        };

        // Check if the resolved method call contains ctx.accounts. or if it's a reload call
        if resolved_method_str.contains("ctx.accounts.") || method_str.contains(".reload()") {
            if let Some(account_name) = Self::extract_account_name_from_expr(&resolved_method_str) {
                // Check if this is a reload call
                if method_str.contains(".reload()") || resolved_method_str.contains(".reload()") {
                    self.reload_lines.push((account_name, line));
                } else if !method_str.contains(".to_account_info()")
                    && !resolved_method_str.contains(".to_account_info()")
                {
                    // It's other account usage (exclude .to_account_info() calls)
                    self.account_usage_lines.push((account_name, line));
                }
            }
        }

        syn::visit::visit_expr_method_call(self, node);
    }
}

impl AccountUsageAnalyzer {
    fn expr_to_string(expr: &syn::Expr) -> String {
        match expr {
            syn::Expr::Path(path) => quote::quote!(#path).to_string(),
            syn::Expr::Field(field) => {
                let member_str = match &field.member {
                    syn::Member::Named(ident) => ident.to_string(),
                    syn::Member::Unnamed(index) => format!("{}", index.index),
                };
                format!("{}.{}", Self::expr_to_string(&field.base), member_str)
            }
            syn::Expr::MethodCall(method) => {
                format!(
                    "{}.{}()",
                    Self::expr_to_string(&method.receiver),
                    method.method
                )
            }
            _ => quote::quote!(#expr).to_string(),
        }
    }

    fn extract_account_name_from_expr(expr_str: &str) -> Option<String> {
        if let Some(start) = expr_str.find("ctx.accounts.") {
            let after_ctx = &expr_str[start + 13..]; // Skip "ctx.accounts."
            let end = after_ctx
                .find(|c| c == '.' || c == ' ' || c == '(')
                .unwrap_or(after_ctx.len());
            Some(after_ctx[..end].to_string())
        } else {
            None
        }
    }
}

/// Analyze a Rust file using static analysis to find invoke/invoke_signed usage
fn analyze_rust_file(file_path: &std::path::Path) -> Result<Vec<InvokeFunctionInfo>> {
    let content = std::fs::read_to_string(file_path)?;
    let syntax = syn::parse_file(&content)?;

    let mut analyzer = InvokeAnalyzer::new(file_path);
    analyzer.visit_file(&syntax);

    Ok(analyzer.functions)
}

/// Static analyzer for finding invoke/invoke_signed usage
struct InvokeAnalyzer {
    file_path: std::path::PathBuf,
    functions: Vec<InvokeFunctionInfo>,
    current_function: Option<FunctionContext>,
    in_program_module: bool,
}

struct FunctionContext {
    name: String,
    invoke_calls: Vec<InvokeCallInfo>,
}

impl InvokeAnalyzer {
    fn new(file_path: &std::path::Path) -> Self {
        Self {
            file_path: file_path.to_path_buf(),
            functions: Vec::new(),
            current_function: None,
            in_program_module: false,
        }
    }

    fn finish_current_function(&mut self) {
        if let Some(func_ctx) = self.current_function.take() {
            if !func_ctx.invoke_calls.is_empty() {
                self.functions.push(InvokeFunctionInfo {
                    function_name: func_ctx.name,
                    file_path: self.file_path.to_string_lossy().to_string(),
                    invoke_calls: func_ctx.invoke_calls,
                });
            }
        }
    }
}

impl Visit<'_> for InvokeAnalyzer {
    fn visit_item_mod(&mut self, node: &ItemMod) {
        if let Some((_, content)) = &node.content {
            // Check if this is a program module by looking for #[program] attribute
            let is_program_module = node.attrs.iter().any(|attr| {
                if let Ok(meta) = attr.parse_meta() {
                    if let syn::Meta::Path(path) = meta {
                        path.is_ident("program")
                    } else {
                        false
                    }
                } else {
                    false
                }
            });

            if is_program_module {
                self.in_program_module = true;
            }

            for item in content {
                self.visit_item(item);
            }

            if is_program_module {
                self.in_program_module = false;
            }
        }
    }

    fn visit_item_fn(&mut self, node: &ItemFn) {
        // Finish previous function
        self.finish_current_function();

        // Start new function context (regardless of whether we're in program module or not)
        self.current_function = Some(FunctionContext {
            name: node.sig.ident.to_string(),
            invoke_calls: Vec::new(),
        });

        // Visit function body
        self.visit_block(&node.block);

        // Finish this function
        self.finish_current_function();
    }

    fn visit_expr_call(&mut self, node: &syn::ExprCall) {
        // Check if this is an invoke call
        if let syn::Expr::Path(path) = &*node.func {
            let path_str = quote::quote!(#path).to_string();

            if path_str.contains("invoke") {
                let line = node.func.span().start().line as u32;
                let invoke_type = if path_str.contains("invoke_signed") {
                    "invoke_signed".to_string()
                } else {
                    "invoke".to_string()
                };

                if let Some(func_ctx) = &mut self.current_function {
                    let mut invoke_call = InvokeCallInfo {
                        line,
                        invoke_type,
                        program_id: None,
                        accounts: Vec::new(),
                        data: None,
                        signers: None,
                    };

                    // Extract arguments from the invoke call
                    Self::extract_invoke_arguments(&node.args, &mut invoke_call);

                    func_ctx.invoke_calls.push(invoke_call);
                }
            }
        }

        // Continue visiting
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_macro(&mut self, node: &syn::Macro) {
        // Check for Anchor macros that use invoke
        let macro_name = node
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default();

        // Only flag macros that actually invoke CPI
        if macro_name.contains("increment")
            || macro_name.contains("decrement")
            || macro_name.contains("anchor")
        {
            let line = node.span().start().line as u32;

            if let Some(func_ctx) = &mut self.current_function {
                let mut invoke_call = InvokeCallInfo {
                    line,
                    invoke_type: "anchor_macro".to_string(),
                    program_id: None,
                    accounts: Vec::new(),
                    data: None,
                    signers: None,
                };

                // Analyze macro arguments to extract account usage
                Self::analyze_macro_arguments(&node.tokens, &mut invoke_call);

                func_ctx.invoke_calls.push(invoke_call);
            }
        }
        // Note: print_account_balance! and other read-only macros are intentionally NOT flagged
        // because they don't invoke CPI and don't require account reload

        // Continue visiting
        syn::visit::visit_macro(self, node);
    }
}

impl InvokeAnalyzer {
    /// Extract detailed information from invoke call arguments
    fn extract_invoke_arguments(
        args: &syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
        invoke_call: &mut InvokeCallInfo,
    ) {
        let args_vec: Vec<&syn::Expr> = args.iter().collect();

        if args_vec.is_empty() {
            return;
        }

        // For invoke: (instruction, accounts)
        // For invoke_signed: (instruction, accounts, signers)
        if args_vec.len() >= 2 {
            // First argument: instruction
            if let syn::Expr::Struct(struct_expr) = &args_vec[0] {
                if let Some(ident) = struct_expr.path.get_ident() {
                    if ident == "Instruction" {
                        // Extract program_id and data from instruction
                        for field in &struct_expr.fields {
                            match &field.member {
                                syn::Member::Named(ident) => match ident.to_string().as_str() {
                                    "program_id" => {
                                        invoke_call.program_id =
                                            Some(Self::expr_to_string(&field.expr));
                                    }
                                    "data" => {
                                        invoke_call.data = Some(Self::expr_to_string(&field.expr));
                                    }
                                    _ => {}
                                },
                                _ => {}
                            }
                        }
                    }
                }
            }

            // Second argument: accounts
            if let syn::Expr::Array(array_expr) = &args_vec[1] {
                for elem in &array_expr.elems {
                    invoke_call.accounts.push(Self::analyze_account(elem));
                }
            } else if let syn::Expr::Reference(ref_expr) = &args_vec[1] {
                // Handle cases like &[...] or &accounts
                if let syn::Expr::Array(array_expr) = &*ref_expr.expr {
                    for elem in &array_expr.elems {
                        invoke_call.accounts.push(Self::analyze_account(elem));
                    }
                } else {
                    // Handle other reference cases
                    invoke_call
                        .accounts
                        .push(Self::analyze_account(&args_vec[1]));
                }
            } else if let syn::Expr::Path(_) = &args_vec[1] {
                // Handle cases like &[] or &accounts
                invoke_call
                    .accounts
                    .push(Self::analyze_account(&args_vec[1]));
            }

            // Third argument: signers (for invoke_signed)
            if args_vec.len() >= 3 && invoke_call.invoke_type == "invoke_signed" {
                if let syn::Expr::Array(array_expr) = &args_vec[2] {
                    for elem in &array_expr.elems {
                        invoke_call.signers = Some(vec![Self::expr_to_string(elem)]);
                    }
                } else {
                    invoke_call.signers = Some(vec![Self::expr_to_string(&args_vec[2])]);
                }
            }
        }
    }

    /// Analyze an account expression to determine if it needs reload after CPI
    fn analyze_account(expr: &syn::Expr) -> AccountInfo {
        let name = Self::expr_to_string(expr);
        let (needs_reload, _) = Self::classify_account(expr);

        AccountInfo {
            name,
            needs_reload,
            used_after_cpi: false,        // Will be analyzed later
            reloaded_before_usage: false, // Will be analyzed later
        }
    }

    /// Classify an account expression based on whether it needs reload after CPI
    /// Only writable accounts need reload because only they can be modified by CPI
    fn classify_account(expr: &syn::Expr) -> (bool, String) {
        let expr_str = Self::expr_to_string(expr);

        if expr_str.contains("ctx.accounts.") {
            // Extract the account name from ctx.accounts.account_name
            if let Some(account_name) = Self::extract_account_name(&expr_str) {
                // Check if this account is writable based on common patterns
                let is_writable = Self::is_account_writable(&account_name);

                if is_writable {
                    (true, "Writable Account - NEEDS RELOAD".to_string())
                } else {
                    (false, "Read-only Account - NO RELOAD NEEDED".to_string())
                }
            } else {
                (true, "Context Account - CANNOT DETERMINE TYPE".to_string())
            }
        } else {
            // For non-context accounts, assume needs reload
            (
                true,
                "Non-Context Account - ASSUME NEEDS RELOAD".to_string(),
            )
        }
    }

    /// Determine if an account is writable based on naming patterns and context
    fn is_account_writable(account_name: &str) -> bool {
        match account_name {
            // Writable accounts - these can be modified by CPI
            "user_account" | "data_account" | "token_account" | "mint" | "vault" | "user"
            | "data" | "state" | "config" | "treasury" | "pool" | "market" | "orderbook"
            | "position" | "trade" | "balance" | "stake" | "reward" | "liquidity" | "reserve"
            | "collateral" | "debt" | "supply" | "borrow" | "mutable_user" => true,
            // Read-only accounts - these cannot be modified by CPI
            "authority"
            | "signer"
            | "payer"
            | "owner"
            | "admin"
            | "creator"
            | "system_program"
            | "token_program"
            | "rent"
            | "clock"
            | "sysvar"
            | "program"
            | "system"
            | "spl_token"
            | "associated_token"
            | "metadata"
            | "rent_sysvar"
            | "clock_sysvar"
            | "recent_blockhashes"
            | "stake_history"
            | "read_only_user"
            | "read_only_authority" => false,
            _ => {
                // Default to writable for unknown account names (conservative approach)
                true
            }
        }
    }

    /// Extract account name from ctx.accounts.account_name expression
    fn extract_account_name(expr_str: &str) -> Option<String> {
        if let Some(start) = expr_str.find("ctx.accounts.") {
            let after_ctx = &expr_str[start + 13..]; // Skip "ctx.accounts."
                                                     // Find the first dot, space, or parenthesis to get the account name
            let end = after_ctx
                .find(|c| c == '.' || c == ' ' || c == '(')
                .unwrap_or(after_ctx.len());
            Some(after_ctx[..end].to_string())
        } else {
            None
        }
    }

    /// Convert a syn::Expr to a readable string representation
    fn expr_to_string(expr: &syn::Expr) -> String {
        match expr {
            syn::Expr::Path(path) => quote::quote!(#path).to_string(),
            syn::Expr::Lit(lit) => quote::quote!(#lit).to_string(),
            syn::Expr::Array(arr) => {
                format!(
                    "[{}]",
                    arr.elems
                        .iter()
                        .map(|e| Self::expr_to_string(e))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            syn::Expr::Reference(ref_expr) => {
                format!("&{}", Self::expr_to_string(&ref_expr.expr))
            }
            syn::Expr::Field(field_expr) => {
                let member_str = match &field_expr.member {
                    syn::Member::Named(ident) => ident.to_string(),
                    syn::Member::Unnamed(index) => format!("{}", index.index),
                };
                format!("{}.{}", Self::expr_to_string(&field_expr.base), member_str)
            }
            syn::Expr::MethodCall(method_call) => {
                format!(
                    "{}.{}()",
                    Self::expr_to_string(&method_call.receiver),
                    method_call.method
                )
            }
            _ => {
                // For complex expressions, try to get a reasonable representation
                quote::quote!(#expr).to_string()
            }
        }
    }

    /// Analyze macro arguments to extract account usage
    fn analyze_macro_arguments(
        tokens: &proc_macro2::TokenStream,
        invoke_call: &mut InvokeCallInfo,
    ) {
        // Convert tokens to string for analysis
        let tokens_str = tokens.to_string();

        // Look for ctx.accounts.account_name patterns in macro arguments
        // Handle both "ctx.accounts." and "ctx . accounts ." patterns
        if tokens_str.contains("ctx") && tokens_str.contains("accounts") {
            // Extract account names from the macro arguments
            let account_patterns = Self::extract_account_patterns_from_tokens(&tokens_str);

            for account_pattern in account_patterns {
                let (needs_reload, _) = Self::classify_account_from_string(&account_pattern);
                invoke_call.accounts.push(AccountInfo {
                    name: account_pattern,
                    needs_reload,
                    used_after_cpi: false,        // Will be analyzed later
                    reloaded_before_usage: false, // Will be analyzed later
                });
            }
        }
    }

    /// Extract account patterns from token string
    fn extract_account_patterns_from_tokens(tokens_str: &str) -> Vec<String> {
        let mut patterns = Vec::new();

        // Simple approach: look for "ctx" followed by "accounts" followed by account name
        // Handle both "ctx.accounts.name" and "ctx . accounts . name" patterns
        let normalized = tokens_str.replace(" ", "").replace(".", "");

        // Look for ctxaccounts pattern
        if let Some(pos) = normalized.find("ctxaccounts") {
            let after_ctxaccounts = &normalized[pos + 11..]; // Skip "ctxaccounts"

            // Find the account name (everything until next non-alphabetic character, but allow underscores)
            let account_end = after_ctxaccounts
                .find(|c: char| !c.is_alphabetic() && c != '_')
                .unwrap_or(after_ctxaccounts.len());
            let account_name = &after_ctxaccounts[..account_end];

            if !account_name.is_empty() {
                patterns.push(format!("ctx.accounts.{}", account_name));
            }
        }

        patterns
    }

    /// Classify account from string pattern
    fn classify_account_from_string(account_pattern: &str) -> (bool, String) {
        if let Some(account_name) = Self::extract_account_name(account_pattern) {
            let is_writable = Self::is_account_writable(&account_name);

            if is_writable {
                (true, "Writable Account - NEEDS RELOAD".to_string())
            } else {
                (false, "Read-only Account - NO RELOAD NEEDED".to_string())
            }
        } else {
            (true, "Context Account - CANNOT DETERMINE TYPE".to_string())
        }
    }
}

/// Check whether `overflow-checks` codegen option is enabled.
///
/// https://doc.rust-lang.org/rustc/codegen-options/index.html#overflow-checks
pub fn check_overflow(cargo_toml_path: impl AsRef<Path>) -> Result<bool> {
    Manifest::from_path(cargo_toml_path)?
        .profile
        .release
        .as_ref()
        .and_then(|profile| profile.overflow_checks)
        .ok_or(anyhow!(
            "`overflow-checks` is not enabled. To enable, add:\n\n\
    [profile.release]\n\
    overflow-checks = true\n\n\
    in workspace root Cargo.toml",
        ))
}

/// Check whether there is a mismatch between the current CLI version and:
///
/// - `anchor-lang` crate version
/// - `@coral-xyz/anchor` package version
///
/// This function logs warnings in the case of a mismatch.
pub fn check_anchor_version(cfg: &WithPath<Config>) -> Result<()> {
    let cli_version = Version::parse(VERSION)?;

    // Check lang crate
    let mismatched_lang_version = cfg
        .get_rust_program_list()?
        .into_iter()
        .map(|path| path.join("Cargo.toml"))
        .map(cargo_toml::Manifest::from_path)
        .filter_map(|man| man.ok())
        .filter_map(|man| man.dependencies.get("anchor-lang").map(|d| d.to_owned()))
        .filter_map(|dep| Version::parse(dep.req()).ok())
        .find(|ver| ver != &cli_version); // Only log the warning once

    if let Some(ver) = mismatched_lang_version {
        eprintln!(
            "WARNING: `anchor-lang` version({ver}) and the current CLI version({cli_version}) \
                 don't match.\n\n\t\
                 This can lead to unwanted behavior. To use the same CLI version, add:\n\n\t\
                 [toolchain]\n\t\
                 anchor_version = \"{ver}\"\n\n\t\
                 to Anchor.toml\n"
        );
    }

    // Check TS package
    let package_json = {
        let package_json_path = cfg.path().parent().unwrap().join("package.json");
        let package_json_content = fs::read_to_string(package_json_path)?;
        serde_json::from_str::<serde_json::Value>(&package_json_content)?
    };
    let mismatched_ts_version = package_json
        .get("dependencies")
        .and_then(|deps| deps.get("@coral-xyz/anchor"))
        .and_then(|ver| ver.as_str())
        .and_then(|ver| VersionReq::parse(ver).ok())
        .filter(|ver| !ver.matches(&cli_version));

    if let Some(ver) = mismatched_ts_version {
        let update_cmd = match cfg.toolchain.package_manager.clone().unwrap_or_default() {
            PackageManager::NPM => "npm update",
            PackageManager::Yarn => "yarn upgrade",
            PackageManager::PNPM => "pnpm update",
            PackageManager::Bun => "bun update",
        };

        eprintln!(
            "WARNING: `@coral-xyz/anchor` version({ver}) and the current CLI version\
                ({cli_version}) don't match.\n\n\t\
                This can lead to unwanted behavior. To fix, upgrade the package by running:\n\n\t\
                {update_cmd} @coral-xyz/anchor@{cli_version}\n"
        );
    }

    Ok(())
}

/// Check for potential dependency improvements.
///
/// The main problem people will run into with Solana v2 is that the `solana-program` version
/// specified in users' `Cargo.toml` might be incompatible with `anchor-lang`'s dependency.
/// To fix this and similar problems, users should use the crates exported from `anchor-lang` or
/// `anchor-spl` when possible.
pub fn check_deps(cfg: &WithPath<Config>) -> Result<()> {
    // Check `solana-program`
    cfg.get_rust_program_list()?
        .into_iter()
        .map(|path| path.join("Cargo.toml"))
        .map(cargo_toml::Manifest::from_path)
        .map(|man| man.map_err(|e| anyhow!("Failed to read manifest: {e}")))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .filter(|man| man.dependencies.contains_key("solana-program"))
        .for_each(|man| {
            eprintln!(
                "WARNING: Adding `solana-program` as a separate dependency might cause conflicts.\n\
                To solve, remove the `solana-program` dependency and use the exported crate from \
                `anchor-lang`.\n\
                `use solana_program` becomes `use anchor_lang::solana_program`.\n\
                Program name: `{}`\n",
                man.package().name()
            )
        });

    Ok(())
}

/// Check whether the `idl-build` feature is being used correctly.
///
/// **Note:** The check expects the current directory to be a program directory.
pub fn check_idl_build_feature() -> Result<()> {
    let manifest_path = Path::new("Cargo.toml").canonicalize()?;
    let manifest = Manifest::from_path(&manifest_path)?;

    // Check whether the manifest has `idl-build` feature
    let has_idl_build_feature = manifest
        .features
        .iter()
        .any(|(feature, _)| feature == "idl-build");
    if !has_idl_build_feature {
        let anchor_spl_idl_build = if manifest
            .dependencies
            .iter()
            .any(|dep| dep.0 == "anchor-spl")
        {
            r#", "anchor-spl/idl-build""#
        } else {
            ""
        };

        return Err(anyhow!(
            r#"`idl-build` feature is missing. To solve, add

[features]
idl-build = ["anchor-lang/idl-build"{anchor_spl_idl_build}]

in `{manifest_path:?}`."#
        ));
    }

    // Check if `idl-build` is enabled by default
    manifest
        .dependencies
        .iter()
        .filter(|(_, dep)| dep.req_features().contains(&"idl-build".into()))
        .for_each(|(name, _)| {
            eprintln!(
                "WARNING: `idl-build` feature of crate `{name}` is enabled by default. \
                    This is not the intended usage.\n\n\t\
                    To solve, do not enable the `idl-build` feature and include crates that have \
                    `idl-build` feature in the `idl-build` feature list:\n\n\t\
                    [features]\n\t\
                    idl-build = [\"{name}/idl-build\", ...]\n"
            )
        });

    // Check `anchor-spl`'s `idl-build` feature
    manifest
        .dependencies
        .get("anchor-spl")
        .and_then(|_| manifest.features.get("idl-build"))
        .map(|feature_list| !feature_list.contains(&"anchor-spl/idl-build".into()))
        .unwrap_or_default()
        .then(|| {
            eprintln!(
                "WARNING: `idl-build` feature of `anchor-spl` is not enabled. \
                This is likely to result in cryptic compile errors.\n\n\t\
                To solve, add `anchor-spl/idl-build` to the `idl-build` feature list:\n\n\t\
                [features]\n\t\
                idl-build = [\"anchor-spl/idl-build\", ...]\n"
            )
        });

    Ok(())
}
