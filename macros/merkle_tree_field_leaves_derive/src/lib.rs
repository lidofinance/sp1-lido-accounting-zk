use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DataStruct};

#[proc_macro_derive(MerkleTreeFieldLeaves)]
pub fn merkle_tree_field_leaves(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();

    impl_merkle_tree_field_leaves(&ast)
}

fn impl_merkle_tree_field_leaves(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let fields_enum_name = format_ident!("{}Fields", name);
    let mut get_leaf_index_entries = Vec::new();
    let mut get_fields_entries = Vec::new();
    let mut fields_enum_entries = Vec::new();

    let struct_data: &DataStruct = match &ast.data {
        Data::Struct(struct_data) => struct_data,
        _ => unimplemented!(),
    };

    let total_fields = struct_data.fields.len();
    for (idx, field) in struct_data.fields.iter().enumerate() {
        let field_name = field.ident.clone().unwrap();
        get_leaf_index_entries.push(quote! { Self::TFields::#field_name => #idx });
        get_fields_entries.push(quote! { self.#field_name.tree_hash_root()});

        fields_enum_entries.push(field_name);
    }

    let gen = quote! {
        #[allow(non_camel_case_types)]
        pub enum #fields_enum_name {
            #(#fields_enum_entries),*
        }

        impl MerkleTreeFieldLeaves for #name {
            const FIELD_COUNT: usize = #total_fields;
            type TFields = #fields_enum_name;

            fn get_leaf_index(field_name: &#fields_enum_name) -> usize {
                match field_name {
                    #(#get_leaf_index_entries,)*
                }
            }

            fn get_fields(&self) -> Vec<Hash256> {
                vec![
                    #(#get_fields_entries),*
                ]
            }
        }
    };
    gen.into()
}
