extern crate proc_macro;

use quote::quote;
use syn::{
    parse::{Parse, ParseStream, Parser},
    spanned::Spanned,
    Ident, Type,
};

fn dependencies() -> proc_macro2::TokenStream {
    quote! {
        use pte::{
            Lines,
        };
    }
}

#[proc_macro_attribute]
pub fn pte(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    pte_impl(attr.into(), item.into()).into()
}

fn pte_impl(
    attr: proc_macro2::TokenStream,
    item: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let dependencies = dependencies();
    let fn_sig = fn_parse.parse2(item.clone()).unwrap();
    let consume_lines =
        fn_sig.to_consume_lines_token_stream(syn::Ident::new("lines", fn_sig.name.span()));
    let fn_sig_declare = fn_sig.to_declare_token_stream();
    let fn_sig_execute = fn_sig.to_execute_token_stream();

    let setup_lines = setup_lines(attr);
    quote! {
        #dependencies

        #fn_sig_declare

        fn main() {
            #setup_lines
            #consume_lines
            let result = #fn_sig_execute
            println!("{}", result);
        }
    }
}
fn setup_lines(attr: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let attr_str = attr.to_string();
    let parse_attr = PteAttrParser::new(&attr_str);
    if parse_attr.exist_row_num_at_input() {
        return setup_lines_by_parse_input(parse_attr);
    }
    let n = parse_attr.get_row_num().unwrap();
    setup_lines_by_row_num(n)
}

fn setup_lines_by_parse_input(parser: PteAttrParser) -> proc_macro2::TokenStream {
    assert!(parser.exist_row_num_at_input());

    let input_ref = parser.get_input_ref().unwrap();
    let input_ref: proc_macro2::Literal = syn::parse_str(&input_ref.to_string()).unwrap();
    quote! {
        let mut first_line = String::new();
        std::io::stdin().read_line(&mut first_line).unwrap();

        let row_num = first_line.split_whitespace().nth(#input_ref).unwrap().parse::<usize>().unwrap();

        let mut input = String::new();
        for _ in 0..row_num {
            std::io::stdin().read_line(&mut input).unwrap();
        }
        let mut lines = Lines::new(&input);
    }
}

fn setup_lines_by_row_num(row_num: isize) -> proc_macro2::TokenStream {
    let row_num_lit = proc_macro2::Literal::isize_unsuffixed(row_num);
    quote! {
        let mut input = String::new();
        for _ in 0..#row_num_lit {
            std::io::stdin().read_line(&mut input).unwrap();
        }
        let mut lines = Lines::new(&input);
    }
}

fn get_vec_type(ty: &Type) -> syn::Result<&Type> {
    let Type::Path(path) = ty else {
        return Err(syn::Error::new(ty.span(), "expected path"));
    };
    let Some(segment) = path.path.segments.first() else {
        return Err(syn::Error::new(ty.span(), "expected segment"));
    };
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return Err(syn::Error::new(ty.span(), "expected angle bracketed"));
    };
    let syn::GenericArgument::Type(ty) = args.args.first().unwrap() else {
        return Err(syn::Error::new(ty.span(), "expected type"));
    };
    Ok(ty)
}

struct FunctionSignature {
    name: Ident,
    args: Vec<(Ident, Type)>,
    return_type: proc_macro2::TokenStream,
    body: syn::Block,
}

impl FunctionSignature {
    fn to_execute_token_stream(&self) -> proc_macro2::TokenStream {
        let name = &self.name;
        let args = self.args.iter().map(|(name, _)| {
            quote! { #name }
        });
        quote! {
            #name(#(#args),*);
        }
    }
    fn to_declare_token_stream(&self) -> proc_macro2::TokenStream {
        let name = &self.name;
        let args = self.args.iter().map(|(name, ty)| {
            quote! { #name: #ty }
        });
        let ty = &self.return_type;
        let body = &self.body;
        quote! {
            fn #name(#(#args),*) #ty #body
        }
    }
    fn to_consume_lines_token_stream(&self, lines_ident: Ident) -> proc_macro2::TokenStream {
        let result = self.args.iter().map(|(name, ty)| {
            if is_vec(ty) {
                let ty = get_vec_type(ty).map_err(|e| e.to_compile_error()).unwrap();
                if is_vec(ty) {
                    let ty = get_vec_type(ty).map_err(|e| e.to_compile_error()).unwrap();
                    return quote! {
                        let #name = #lines_ident.consume_to_two_d_vec::<#ty>().unwrap();
                    };
                }
                return quote! {
                    let #name = #lines_ident.consume_to_vec::<#ty>().unwrap();
                };
            }
            quote! {
                let #name = #lines_ident.consume::<#ty>().unwrap();
            }
        });
        quote! {
            #(#result)*
        }
    }
}

fn fn_parse(input: ParseStream) -> syn::Result<FunctionSignature> {
    FunctionSignature::parse(input)
}

impl Parse for FunctionSignature {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let _fn: syn::Token![fn] = input.parse()?;
        let name: Ident = input.parse()?;
        let content;
        let _parentheses = syn::parenthesized!(content in input);
        let args = content
            .parse_terminated::<_, syn::Token![,]>(|input| {
                let name: Ident = input.parse()?;
                let _colon: syn::Token![:] = input.parse()?;
                let ty: Type = input.parse()?;
                Ok((name, ty))
            })?
            .into_iter()
            .collect();

        let return_type = if input.peek(syn::Token![->]) {
            let _arrow: syn::Token![->] = input.parse()?;
            let return_type: Type = input.parse()?;
            quote! { -> #return_type }
        } else {
            quote! {}
        };
        let body: syn::Block = input.parse().map_err(|e| {
            syn::Error::new(
                e.span(),
                format!(
                    "expected block expression for function body {}",
                    input.to_string()
                ),
            )
        })?;

        Ok(Self {
            name,
            args,
            return_type,
            body,
        })
    }
}

fn is_vec(ty: &Type) -> bool {
    if let Type::Path(path) = ty {
        if let Some(segment) = path.path.segments.first() {
            if segment.ident == "Vec" {
                return true;
            }
        }
    }
    false
}

#[derive(Debug)]
struct PteAttrParser<'a> {
    attr: &'a str,
}

impl PteAttrParser<'_> {
    const ROW_KEY: &'static str = "row";
    fn new(attr: &str) -> PteAttrParser {
        PteAttrParser { attr }
    }
    fn exist_row_num_at_input(&self) -> bool {
        self.attr.contains(Self::ROW_KEY)
    }
    // get by default or row = NUMBER
    fn get_row_num(&self) -> Result<isize, String> {
        if self.attr == "" || !self.attr.contains(Self::ROW_KEY) {
            return Ok(self.default_row_num());
        }
        let row_value = self.get_row_attr_value();
        row_value.parse::<isize>().map_err(|e| e.to_string())
    }

    fn get_input_ref(&self) -> Result<usize, String> {
        if self.attr == "" || !self.attr.contains(Self::ROW_KEY) {
            return Err("input reference not found".to_string());
        }
        let row_value = self.get_row_attr_value();
        parse_input_ref(row_value)
    }

    fn get_row_attr_value(&self) -> &str {
        if self.attr == "" || !self.attr.contains(Self::ROW_KEY) {
            return "";
        }
        let mut attrs = self.attr.split(",");
        let row_attr = attrs
            .find(|attr| attr.contains(Self::ROW_KEY))
            .unwrap_or_default();
        let split = row_attr.split("=");
        split.last().unwrap_or_default().trim()
    }
    fn default_row_num(&self) -> isize {
        1
    }
}

fn parse_input_ref(input_ref: &str) -> Result<usize, String> {
    fn error_msg(v: &str) -> String {
        format!("invalid input reference {}, format is \"inNUMBER\"", v)
    }
    let Some("in") = input_ref.get(0..2) else {
        return Err(error_msg(input_ref));
    };
    let Ok(result) = input_ref[2..3].parse::<usize>() else {
        return Err(error_msg(input_ref));
    };
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pte_test() {
        let attr = quote! { row = in1 };
        let item = quote! {
            fn solve(v: usize) -> i32 {
            }
        };
        let got = pte_impl(attr, item);

        let expect = quote! {
            use pte::{
                Lines,
            };
            fn solve(v:usize) -> i32 {
            }
            fn main() {
                let mut first_line = String::new();
                std::io::stdin().read_line(&mut first_line).unwrap();

                let row_num = first_line.split_whitespace().nth(1).unwrap().parse::<usize>().unwrap();

                let mut input = String::new();
                for _ in 0..row_num {
                    std::io::stdin().read_line(&mut input).unwrap();
                }
                let mut lines = Lines::new(&input);
                let v = lines.consume::<usize>().unwrap();
                let result = solve(v);
                println!("{}", result);
            }
        };
        assert_eq!(got.to_string(), expect.to_string());
    }

    #[test]
    fn parse_attr_row_from_input() {
        let attr = "row = in0, column = in1";
        let sut = PteAttrParser::new(attr);
        assert!(sut.exist_row_num_at_input());
        let got = sut.get_input_ref().unwrap();
        assert_eq!(got, 0);
    }
    #[test]
    fn parse_attr_row_default() {
        let attr = "";
        let sut = PteAttrParser::new(attr);

        assert!(!sut.exist_row_num_at_input());
        let got = sut.get_row_num().unwrap();
        assert_eq!(got, 1);
    }
    #[test]
    fn setup_lines() {
        let n = 3;
        let got = setup_lines_by_row_num(n);
        let expect = quote! {
            let mut input = String::new();
            for _ in 0..3 {
                std::io::stdin().read_line(&mut input).unwrap();
            }
            let mut lines = Lines::new(&input);
        };
        assert_eq!(got.to_string(), expect.to_string());

        let attr = "row = in0";
        let parser = PteAttrParser::new(attr);
        let got = setup_lines_by_parse_input(parser);

        let expect = quote! {
            let mut first_line = String::new();
            std::io::stdin().read_line(&mut first_line).unwrap();

            let row_num = first_line.split_whitespace().nth(0).unwrap().parse::<usize>().unwrap();

            let mut input = String::new();
            for _ in 0..row_num {
                std::io::stdin().read_line(&mut input).unwrap();
            }
            let mut lines = Lines::new(&input);
        };
        assert_eq!(got.to_string(), expect.to_string());
    }
}
