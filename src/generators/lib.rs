use crate::ffi;
use crate::models::Pointer::DoublePointer;
use crate::models::Type::{FundamentalType, UserType};
use crate::models::{Api, Argument, Enumeration, Error, Field, Function, Pointer, Structure, Type};
use convert_case::{Case, Casing};
use quote::__private::{Ident, TokenStream};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;

use crate::generators::dictionary::{KEYWORDS, RENAMES};

#[derive(Debug, Clone, PartialEq)]
pub struct Struct {
    pub structure: Structure,
    pub constructor: Function,
    pub methods: Vec<Function>,
}

#[derive(Debug, Default)]
pub struct Lib {
    pub structs: Vec<Struct>,
}

fn extract_struct_key(name: &str) -> String {
    match name.rfind('_') {
        Some(index) => name[..index].to_uppercase(),
        None => name.to_string(),
    }
}

const ENUMERATOR_RENAMES: &[(&str, &str)] = &[
    ("FMOD_STUDIO_LOAD_MEMORY", "FMOD_STUDIO_LOAD_MEMORY_MMEMORY"),
    (
        "FMOD_STUDIO_LOAD_MEMORY_POINT",
        "FMOD_STUDIO_LOAD_MEMORY_MMEMORY_POINT",
    ),
];

fn format_enumerator_ident(enumeration: &str, name: &str) -> Ident {
    let name = match ENUMERATOR_RENAMES.iter().find(|pair| pair.0 == name) {
        None => name,
        Some(pair) => pair.1,
    };
    let mut p = 0;
    while p < name.len() && p < enumeration.len() {
        if enumeration.chars().nth(p) != name.chars().nth(p) {
            break;
        }
        p += 1;
    }
    let key = (&name[p..]).to_case(Case::UpperCamel);

    let key = if key.chars().nth(0).unwrap_or('a').is_ascii_digit() {
        format!("_{}", key)
    } else {
        key
    };
    let name = key;
    let name = match RENAMES.get(&name[..]) {
        None => name,
        Some(rename) => rename.to_string(),
    };
    format_ident!("{}", name)
}

fn extract_method_name(name: &str) -> String {
    match name.rfind('_') {
        Some(index) => name[index..].to_string().to_case(Case::Snake),
        None => name.to_string(),
    }
}

fn format_struct_ident(key: &str) -> Ident {
    let key = key.replace("FMOD_RESULT", "FMOD_FMODRESULT");
    let key = key.replace("FMOD_", "");
    let key = key.replace("STUDIO_SYSTEM", "STUDIOSYSTEM");
    let key = key.replace("STUDIO_ADVANCEDSETTINGS", "STUDIOADVANCEDSETTINGS");
    let key = key.replace("STUDIO_CPU_USAGE", "STUDIOCPUUSAGE");
    let key = key.replace("STUDIO_", "");
    let name = key.to_case(Case::Pascal);
    let name = match RENAMES.get(&name[..]) {
        None => name,
        Some(rename) => rename.to_string(),
    };
    format_ident!("{}", name)
}

pub fn format_argument_ident(name: &str) -> Ident {
    let name = name.to_case(Case::Snake);
    if KEYWORDS.contains(&&*name) {
        format_ident!("{}_", name)
    } else {
        format_ident!("{}", name)
    }
}

pub fn generate_struct_method_code(method: &Function) -> TokenStream {
    quote! {
        pub fn load_bank_file(
            &self,
            filename: &str,
            flags: FMOD_STUDIO_LOAD_BANK_FLAGS,
        ) -> Result<Bank, MyError> {
            let mut pointer = null_mut();
            let filename = CString::new(filename).unwrap();
            let result = unsafe {
                FMOD_Studio_System_LoadBankFile(
                    self.pointer,
                    filename.as_ptr(),
                    flags,
                    &mut pointer,
                )
            };
            if result == FMOD_OK {
                Ok(pointer.into())
            } else {
                Err(MyError("FMOD_Studio_System_LoadBankFile".into(), decode_error(result).to_string()))
            }
        }
    }
}

pub fn is_normal(pointer: &Option<Pointer>) -> bool {
    if let Some(Pointer::NormalPointer(_)) = pointer {
        true
    } else {
        false
    }
}

pub fn is_double(pointer: &Option<Pointer>) -> bool {
    if let Some(Pointer::DoublePointer(_)) = pointer {
        true
    } else {
        false
    }
}

pub fn describe_ffi_pointer<'a>(
    as_const: &'a Option<String>,
    pointer: &'a Option<Pointer>,
) -> &'a str {
    let description = match (as_const, pointer) {
        (None, None) => "",
        (None, Some(Pointer::NormalPointer(_))) => "*mut",
        (None, Some(Pointer::DoublePointer(_))) => "*mut *mut",
        (Some(_), Some(Pointer::NormalPointer(_))) => "*const",
        (Some(_), Some(Pointer::DoublePointer(_))) => "*const *const",
        (Some(_), None) => "",
    };
    description
}

pub fn format_rust_type(
    c_type: &Type,
    as_const: &Option<String>,
    pointer: &Option<Pointer>,
    as_array: &Option<TokenStream>,
    api: &Api,
) -> TokenStream {
    let ptr = describe_ffi_pointer(as_const, pointer);
    let tokens = match c_type {
        FundamentalType(name) => match (ptr, &name[..]) {
            ("*const", "char") => quote! { String },
            ("*const *const", "char") => quote! { Vec<String> },
            ("*mut", "char") => quote! { String },
            ("*mut", "void") => quote! { *mut c_void },
            ("*mut", "int") => quote! { Vec<i32> },
            ("*mut", "float") => quote! { Vec<f32> },
            ("*mut *mut", "float") => quote! { Vec<f32> },
            ("*mut *mut", "char") => quote! { Vec<String> },
            ("", "unsigned char") => quote! { u8 },
            ("", "char") => quote! { i8 },
            ("", "int") => quote! { i32 },
            ("", "unsigned int") => quote! { u32 },
            ("", "short") => quote! { i16 },
            ("", "unsigned short") => quote! { u16 },
            ("", "long long") => quote! { i64 },
            ("", "long") => quote! { i64 },
            ("", "unsigned long long") => quote! { u64 },
            ("", "unsigned long") => quote! { u64 },
            ("", "float") => quote! { f32 },
            _ => {
                let name = format_ident!("{}", name);
                quote! { Box<#name> }
            }
        },
        UserType(name) => match (ptr, api.describe_user_type(name)) {
            ("*mut", UserTypeDesc::OpaqueType) => {
                let name = format_struct_ident(name);
                quote! { #name }
            }
            ("*mut", UserTypeDesc::Structure) => {
                let name = format_struct_ident(name);
                quote! { #name }
            }
            ("*mut *mut", UserTypeDesc::Structure) => {
                let name = format_ident!("{}", name);
                quote! { Vec<ffi::#name> }
            }
            ("*mut", UserTypeDesc::Flags) => {
                let name = format_ident!("{}", name);
                quote! { Vec<ffi::#name> }
            }
            ("*mut", UserTypeDesc::Enumeration) => {
                let name = format_struct_ident(name);
                quote! { Vec<#name> }
            }
            ("", UserTypeDesc::Structure) => {
                let name = format_struct_ident(name);
                quote! { #name }
            }
            ("", UserTypeDesc::Enumeration) => {
                let name = format_struct_ident(name);
                quote! { #name }
            }
            ("", _) => {
                let name = format_ident!("{}", name);
                quote! { ffi::#name }
            }
            _ => quote! { err },
        },
    };
    match as_array {
        None => tokens,
        Some(dimension) => {
            quote! {
                [#tokens; #dimension as usize]
            }
        }
    }
}

pub fn generate_argument_code(argument: &Argument, api: &Api) -> TokenStream {
    let name = format_argument_ident(&argument.name);
    let argument_type = format_rust_type(
        &argument.argument_type,
        &argument.as_const,
        &argument.pointer,
        &None,
        &api,
    );
    quote! {
        #name: #argument_type
    }
}

pub fn generate_argument_map_code(argument: &Argument) -> TokenStream {
    let name = format_argument_ident(&argument.name);
    quote! { #name }
}

pub fn generate_enumeration_code(enumeration: &Enumeration) -> TokenStream {
    let name = format_struct_ident(&enumeration.name);

    let mut keys = vec![];
    let mut input_map = vec![];
    let mut output_map = vec![];

    for enumerator in &enumeration.enumerators {
        if enumerator.name.ends_with("FORCEINT") {
            // skip unused workaround
            continue;
        }
        let key = format_enumerator_ident(&enumeration.name, &enumerator.name);
        let enumerator = format_ident!("{}", enumerator.name);
        input_map.push(quote! {#name::#key => ffi::#enumerator});
        output_map.push(quote! {ffi::#enumerator => Ok(#name::#key)});
        keys.push(key);
    }

    let enumeration_name = &enumeration.name;
    let enumeration = format_ident!("{}", enumeration_name);

    quote! {
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub enum #name {
            #(#keys),*
        }

        impl From<#name> for ffi::#enumeration {
            fn from(value: #name) -> ffi::#enumeration {
                match value {
                    #(#input_map),*
                }
            }
        }

        impl #name {
            pub fn from(value: ffi::#enumeration) -> Result<#name, Error> {
                match value {
                    #(#output_map),*,
                    _ => Err(err_enum!(#enumeration_name, value)),
                }
            }
        }
    }
}

pub fn generate_field_code(field: &Field, api: &Api) -> Result<TokenStream, Error> {
    let name = format_argument_ident(&field.name);
    let as_array = match &field.as_array {
        None => None,
        Some(dimension) => {
            let token = &dimension[1..dimension.len() - 1];
            let dimension = match api.describe_user_type(token) {
                UserTypeDesc::Constant => {
                    let name = format_ident!("{}", token);
                    quote! { ffi::#name }
                }
                _ => TokenStream::from_str(token)?,
            };
            Some(dimension)
        }
    };
    let field_type = format_rust_type(
        &field.field_type,
        &field.as_const,
        &field.pointer,
        &as_array,
        &api,
    );
    Ok(quote! {
        pub #name: #field_type
    })
}

pub fn generate_field_from_code(
    structure: &str,
    field: &Field,
    api: &Api,
) -> Result<TokenStream, Error> {
    let name = format_argument_ident(&field.name);
    let value_name = ffi::format_rust_ident(&field.name);
    let ptr = describe_ffi_pointer(&field.as_const, &field.pointer);

    let getter = match (structure, &field.name[..]) {
        ("FMOD_DSP_PARAMETER_3DATTRIBUTES_MULTI", "relative") => {
            quote! { attr3d_array8(value.relative.map(Attributes3d::from).into_iter().collect::<Result<Vec<Attributes3d>, Error>>()?) }
        }
        ("FMOD_CREATESOUNDEXINFO", "inclusionlist") => {
            quote! { to_vec!(value.inclusionlist, value.inclusionlistnum) }
        }
        ("FMOD_ADVANCEDSETTINGS", "ASIOChannelList") => {
            quote! { to_vec!(value.ASIOChannelList, value.ASIONumChannels, |ptr| to_string!(ptr))? }
        }
        ("FMOD_ADVANCEDSETTINGS", "ASIOSpeakerList") => {
            quote! { to_vec!(value.ASIOSpeakerList, value.ASIONumChannels, Speaker::from)? }
        }
        ("FMOD_OUTPUT_OBJECT3DINFO", "buffer") => {
            quote! { to_vec!(value.buffer, value.bufferlength) }
        }
        ("FMOD_DSP_BUFFER_ARRAY", "buffernumchannels") => {
            quote! { to_vec!(value.buffernumchannels, value.numbuffers) }
        }
        ("FMOD_DSP_BUFFER_ARRAY", "bufferchannelmask") => {
            quote! { to_vec!(value.bufferchannelmask, value.numbuffers) }
        }
        ("FMOD_DSP_BUFFER_ARRAY", "buffers") => {
            quote! { to_vec!(value.buffers, value.numbuffers, |ptr| Ok(*ptr))? }
        }
        ("FMOD_DSP_PARAMETER_FLOAT_MAPPING_PIECEWISE_LINEAR", "pointparamvalues") => {
            quote! { to_vec!(value.pointparamvalues, value.numpoints) }
        }
        ("FMOD_DSP_PARAMETER_FLOAT_MAPPING_PIECEWISE_LINEAR", "pointpositions") => {
            quote! { to_vec!(value.pointpositions, value.numpoints) }
        }
        ("FMOD_DSP_PARAMETER_DESC_INT", "valuenames") => {
            quote! { vec![] } // TODO
        }
        ("FMOD_DSP_PARAMETER_DESC_BOOL", "valuenames") => {
            quote! { vec![] } // TODO
        }
        ("FMOD_DSP_PARAMETER_FFT", "spectrum") => {
            quote! { value.spectrum.map(|ptr| to_vec!(ptr, value.numchannels)) }
        }
        ("FMOD_DSP_DESCRIPTION", "paramdesc") => {
            quote! { vec![] } // TODO
        }
        ("FMOD_DSP_STATE", "sidechaindata") => {
            quote! { to_vec!(value.sidechaindata, value.sidechainchannels) }
        }
        _ => match &field.field_type {
            FundamentalType(name) => match (ptr, &name[..]) {
                ("*const", "char") => quote! { to_string!(value.#value_name)? },
                ("*mut", "char") => quote! { to_string!(value.#value_name)? },
                _ => quote! { value.#value_name },
            },
            UserType(name) => match (ptr, api.describe_user_type(name)) {
                ("*mut", UserTypeDesc::OpaqueType) => {
                    let name = format_struct_ident(name);
                    quote! { #name::from_pointer(value.#value_name) }
                }
                ("*mut", UserTypeDesc::Structure) => {
                    let name = format_struct_ident(name);
                    quote! { #name::from(*value.#value_name)? }
                }
                ("", UserTypeDesc::Structure) => {
                    let name = format_struct_ident(name);
                    quote! { #name::from(value.#value_name)? }
                }
                ("", UserTypeDesc::Enumeration) => {
                    let name = format_struct_ident(name);
                    quote! { #name::from(value.#value_name)? }
                }
                _ => quote! { value.#value_name },
            },
        },
    };

    Ok(quote! {#name: #getter})
}

pub fn generate_field_into_code(
    structure: &str,
    field: &Field,
    api: &Api,
) -> Result<TokenStream, Error> {
    let name = ffi::format_rust_ident(&field.name);
    let self_name = format_argument_ident(&field.name);
    let ptr = describe_ffi_pointer(&field.as_const, &field.pointer);

    let getter = match (structure, &field.name[..]) {
        ("FMOD_DSP_PARAMETER_3DATTRIBUTES_MULTI", "relative") => {
            quote! { self.relative.map(Attributes3d::into) }
        }
        ("FMOD_CREATESOUNDEXINFO", "inclusionlist") => {
            quote! { self.inclusionlist.as_ptr() as *mut _ }
        }
        ("FMOD_OUTPUT_OBJECT3DINFO", "buffer") => {
            quote! { self.buffer.as_ptr() as *mut _ }
        }
        ("FMOD_ADVANCEDSETTINGS", "ASIOChannelList") => {
            quote! { self.asio_channel_list.into_iter().map(|val| val.as_ptr()).collect::<Vec<_>>().as_mut_ptr().cast() }
        }
        ("FMOD_ADVANCEDSETTINGS", "ASIOSpeakerList") => {
            quote! { self.asio_speaker_list.into_iter().map(|val| val.into()).collect::<Vec<_>>().as_mut_ptr() }
        }
        ("FMOD_DSP_BUFFER_ARRAY", "buffernumchannels") => {
            quote! { self.buffernumchannels.as_ptr() as *mut _ }
        }
        ("FMOD_DSP_BUFFER_ARRAY", "bufferchannelmask") => {
            quote! { self.bufferchannelmask.as_ptr() as *mut _ }
        }
        ("FMOD_DSP_BUFFER_ARRAY", "buffers") => {
            quote! { self.buffers.as_ptr() as *mut _ }
        }
        ("FMOD_DSP_PARAMETER_FLOAT_MAPPING_PIECEWISE_LINEAR", "pointparamvalues") => {
            quote! { self.pointparamvalues.as_ptr() as *mut _ }
        }
        ("FMOD_DSP_PARAMETER_FLOAT_MAPPING_PIECEWISE_LINEAR", "pointpositions") => {
            quote! { self.pointpositions.as_ptr() as *mut _ }
        }
        ("FMOD_DSP_PARAMETER_DESC_INT", "valuenames") => {
            quote! { self.valuenames.as_ptr() as *mut _ }
        }
        ("FMOD_DSP_PARAMETER_DESC_BOOL", "valuenames") => {
            quote! { self.valuenames.as_ptr() as *mut _ }
        }
        ("FMOD_DSP_PARAMETER_FFT", "spectrum") => {
            quote! { self.spectrum.map(|val| val.as_ptr() as *mut _) }
        }
        ("FMOD_DSP_DESCRIPTION", "paramdesc") => {
            quote! { null_mut() } // TODO
        }
        ("FMOD_DSP_STATE", "sidechaindata") => {
            quote! { self.sidechaindata.as_ptr() as *mut _ }
        }
        _ => match &field.field_type {
            FundamentalType(name) => match (ptr, &name[..]) {
                ("*const", "char") => quote! { self.#self_name.as_ptr().cast() },
                ("*mut", "char") => quote! { self.#self_name.as_ptr() as *mut _ },
                _ => quote! { self.#self_name },
            },
            UserType(name) => match (ptr, api.describe_user_type(name)) {
                ("*mut", UserTypeDesc::OpaqueType) => {
                    quote! { self.#self_name.as_mut_ptr() }
                }
                ("*mut", UserTypeDesc::Structure) => {
                    quote! { &mut self.#self_name.into() }
                }
                ("", UserTypeDesc::Structure) => {
                    quote! { self.#self_name.into() }
                }
                ("", UserTypeDesc::Enumeration) => {
                    quote! { self.#self_name.into() }
                }
                _ => quote! { self.#self_name },
            },
        },
    };

    Ok(quote! {#name: #getter})
}

pub fn generate_structure_code(structure: &Structure, api: &Api) -> Result<TokenStream, Error> {
    let structure_name = format_ident!("{}", structure.name);
    let name = format_struct_ident(&structure.name);

    let mut fields = vec![];
    let mut from_map = vec![];
    let mut into_map = vec![];
    for field in &structure.fields {
        fields.push(generate_field_code(field, api)?);
        from_map.push(generate_field_from_code(&structure.name, field, api)?);
        into_map.push(generate_field_into_code(&structure.name, field, api)?);
    }

    if let Some(union) = &structure.union {
        let name = format_ident!("{}__union", structure.name);
        fields.push(quote! {
            pub __union: ffi::#name
        });
        from_map.push(quote! { __union: value.__union });
        into_map.push(quote! { __union: self.__union });
    }

    let debug = if structure.union.is_some() || ["FMOD_DSP_DESCRIPTION"].contains(&&*structure.name)
    {
        None
    } else {
        Some(quote! {Debug,})
    };

    Ok(quote! {
        #[derive(#debug Clone)]
        pub struct #name {
            #(#fields),*
        }

        impl #name {
            pub fn from(value: ffi::#structure_name) -> Result<#name, Error> {
                unsafe {
                    Ok(#name {
                        #(#from_map),*
                    })
                }
            }
            pub fn into(self) -> ffi::#structure_name {
                ffi::#structure_name {
                    #(#into_map),*
                }
            }
        }
    })
}

pub fn generate_method_code(owner: &str, function: &Function, api: &Api) -> TokenStream {
    let mut arguments = vec![];
    let mut output = None;

    for argument in function.arguments.clone() {
        if argument.argument_type == UserType(owner.into()) && is_normal(&argument.pointer) {
            continue;
        } else if is_double(&argument.pointer) && output.is_none() {
            output = Some(argument)
        } else {
            arguments.push(argument);
        }
    }

    let argument_maps: Vec<TokenStream> =
        arguments.iter().map(generate_argument_map_code).collect();
    let arguments: Vec<TokenStream> = arguments
        .iter()
        .map(|argument| generate_argument_code(argument, api))
        .collect();

    let method = format_ident!(
        "{}",
        extract_method_name(&function.name).to_case(Case::Snake)
    );
    let function_name = &function.name;
    let function = format_ident!("{}", function_name);

    match output {
        Some(_) => quote! {
            pub fn #method(&self, #(#arguments),*) -> Result<Bank, Error> {
                let mut output = null_mut();
                let filename = CString::new(filename).unwrap();
                let result = unsafe {
                    ffi::#function(self.pointer, &mut output)
                };
                if result == FMOD_OK {
                    Ok(output.into())
                } else {
                    Err(err_fmod!(#function_name, result))
                }
            }
        },
        None => quote! {
            pub fn #method(&self, #(#arguments),*) -> Result<(), Error> {
                let result = unsafe {
                    ffi::#function(self.pointer, #(#argument_maps),*)
                };
                if result == FMOD_OK {
                    Ok(())
                } else {
                    Err(err_fmod!(#function_name, result))
                }
            }
        },
    }
}

pub fn generate_opaque_type_code(key: &String, methods: &Vec<&Function>) -> TokenStream {
    let name = format_struct_ident(key);
    let opaque_type = format_ident!("{}", key);

    let constructor = match methods.iter().find(|method| {
        method.name.ends_with("Create")
            && method.arguments.iter().any(|argument| {
                argument.argument_type == UserType(key.clone())
                    && argument.pointer == Some(DoublePointer("**".into()))
            })
    }) {
        None => None,
        Some(function) => {
            let name = format_ident!("{}", extract_method_name(&function.name));
            let function_name = &function.name;
            let function = format_ident!("{}", function_name);
            Some(quote! {
                pub fn #name() -> Result<Self, Error> {
                    let mut pointer = null_mut();
                    let result = unsafe {
                        ffi::#function(&mut pointer, ffi::FMOD_VERSION)
                    };
                    if result == ffi::FMOD_OK {
                        Ok(Self { pointer })
                    } else {
                        Err(err_fmod!(#function_name, result))
                    }
                }
            })
        }
    };

    quote! {
        #[derive(Debug, Clone, Copy)]
        pub struct #name {
            pointer: *mut ffi::#opaque_type,
        }

        impl #name {
            #constructor
            pub fn from_pointer(pointer: *mut ffi::#opaque_type) -> Self {
                Self { pointer }
            }
            pub fn as_mut_ptr(&self) -> *mut ffi::#opaque_type {
                self.pointer
            }
        }
    }
}

enum UserTypeDesc {
    OpaqueType,
    Structure,
    Enumeration,
    Flags,
    Constant,
    Unknown,
}

impl Api {
    pub fn is_structure(&self, key: &str) -> bool {
        self.structures
            .iter()
            .any(|structure| &structure.name == key)
    }

    pub fn is_opaque_type(&self, key: &str) -> bool {
        self.opaque_types
            .iter()
            .any(|opaque_type| &opaque_type.name == key)
    }

    pub fn is_enumeration(&self, key: &str) -> bool {
        self.enumerations
            .iter()
            .any(|enumeration| &enumeration.name == key)
    }

    pub fn is_flags(&self, key: &str) -> bool {
        self.flags.iter().any(|flags| &flags.name == key)
    }

    pub fn is_constant(&self, key: &str) -> bool {
        self.constants.iter().any(|constant| &constant.name == key)
    }

    pub fn describe_user_type(&self, key: &str) -> UserTypeDesc {
        if self.is_structure(key) {
            UserTypeDesc::Structure
        } else if self.is_enumeration(key) {
            UserTypeDesc::Enumeration
        } else if self.is_flags(key) {
            UserTypeDesc::Flags
        } else if self.is_opaque_type(key) {
            UserTypeDesc::OpaqueType
        } else if self.is_constant(key) {
            UserTypeDesc::Constant
        } else {
            UserTypeDesc::Unknown
        }
    }
}

pub fn generate_lib_code(api: &Api) -> Result<TokenStream, Error> {
    let functions: Vec<&Function> = api
        .functions
        .iter()
        .flat_map(|(_, functions)| functions)
        .collect();

    let opaque_types = api
        .opaque_types
        .iter()
        .map(|opaque_type| opaque_type.name.clone());
    let opaque_types: HashSet<String> = HashSet::from_iter(opaque_types);

    let mut types: BTreeMap<String, Vec<&Function>> = BTreeMap::new();
    for function in &functions {
        let key = extract_struct_key(&function.name);
        if opaque_types.contains(&key) {
            match types.get_mut(&key) {
                Some(methods) => methods.push(function),
                None => {
                    types.insert(key, vec![function]);
                }
            }
        } else {
            println!("Global function: {}", function.name);
        }
    }

    let types: Vec<TokenStream> = types
        .iter()
        .map(|(key, methods)| generate_opaque_type_code(key, methods))
        .collect();

    let enumerations: Vec<TokenStream> = api
        .enumerations
        .iter()
        .map(generate_enumeration_code)
        .collect();

    let mut structures: Vec<TokenStream> = vec![];
    for structure in &api.structures {
        structures.push(generate_structure_code(structure, api)?);
    }

    Ok(quote! {
        #![allow(unused_unsafe)]
        use std::ffi::{c_void, CStr, CString, IntoStringError};
        use std::ptr::null_mut;
        use std::slice;
        pub mod ffi;

        #[derive(Debug)]
        pub enum Error {
            Fmod {
                function: String,
                code: i32,
                message: String,
            },
            EnumBindgen {
                enumeration: String,
                value: String
            },
            String(IntoStringError)
        }

        macro_rules! err_fmod {
            ($ function : expr , $ code : expr) => {
                Error::Fmod {
                    function: $function.to_string(),
                    code: $code,
                    message: ffi::map_fmod_error($code).to_string(),
                }
            };
        }

        macro_rules! err_enum {
            ($ enumeration : expr , $ value : expr) => {
                Error::EnumBindgen {
                    enumeration: $enumeration.to_string(),
                    value: $value.to_string(),
                }
            };
        }

        macro_rules! to_string {
            ($ptr:expr) => {
                CString::from(CStr::from_ptr($ptr)).into_string().map_err(Error::String)
            };
        }

        macro_rules! to_vec {
            ($ ptr : expr , $ length : expr, $ closure : expr) => {
                slice::from_raw_parts($ptr, $length as usize).to_vec().into_iter().map($closure).collect::<Result<Vec<_>, Error>>()
            };
            ($ ptr : expr , $ length : expr) => {
                slice::from_raw_parts($ptr, $length as usize).to_vec()
            };
        }

        pub fn attr3d_array8(values: Vec<Attributes3d>) -> [Attributes3d; ffi::FMOD_MAX_LISTENERS as usize] {
            values.try_into().expect("slice with incorrect length")
        }

        #(#enumerations)*
        #(#structures)*
        #(#types)*
    })
}

pub fn generate(api: &Api) -> Result<String, Error> {
    let code = generate_lib_code(api)?;
    rustfmt_wrapper::rustfmt(code).map_err(Error::from)
}

#[cfg(test)]
mod tests {
    use crate::lib::{generate_enumeration_code, generate_method_code, generate_structure_code};
    use crate::models::Type::{FundamentalType, UserType};
    use crate::models::{Argument, Enumeration, Enumerator, Field, Function, Pointer, Structure};

    fn normal() -> Option<Pointer> {
        Some(Pointer::NormalPointer("*".into()))
    }

    #[test]
    fn test_should_generate_simple_arguments_method() {
        let function = Function {
            return_type: UserType("FMOD_RESULT".into()),
            name: "FMOD_System_SetDSPBufferSize".to_string(),
            arguments: vec![
                Argument {
                    as_const: None,
                    argument_type: UserType("FMOD_SYSTEM".into()),
                    pointer: normal(),
                    name: "system".to_string(),
                },
                Argument {
                    as_const: None,
                    argument_type: FundamentalType("unsigned int".into()),
                    pointer: None,
                    name: "bufferlength".to_string(),
                },
                Argument {
                    as_const: None,
                    argument_type: FundamentalType("int".into()),
                    pointer: None,
                    name: "numbuffers".to_string(),
                },
            ],
        };
        let actual = generate_method_code("FMOD_SYSTEM", &function).to_string();
        let expected = quote! {
            pub fn set_dsp_buffer_size(&self, bufferlength: u32, numbuffers: i32) -> Result<(), Error> {
                let result = unsafe {
                    ffi::FMOD_System_SetDSPBufferSize(self.pointer, bufferlength, numbuffers)
                };
                if result == FMOD_OK {
                    Ok(())
                } else {
                    Err(err_fmod!("FMOD_System_SetDSPBufferSize", result))
                }
            }
        }.to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_should_generate_enumeration() {
        let enumeration = Enumeration {
            name: "FMOD_OUTPUTTYPE".to_string(),
            enumerators: vec![
                Enumerator {
                    name: "FMOD_OUTPUTTYPE_AUTODETECT".to_string(),
                    value: None,
                },
                Enumerator {
                    name: "FMOD_OUTPUTTYPE_UNKNOWN".to_string(),
                    value: None,
                },
                Enumerator {
                    name: "FMOD_OUTPUTTYPE_FORCEINT".to_string(),
                    value: Some("65536".into()),
                },
            ],
        };
        let actual = generate_enumeration_code(&enumeration).to_string();
        let expected = quote! {
            #[derive(Debug, Clone, Copy, PartialEq)]
            pub enum OutputType {
                Autodetect,
                Unknown
            }

            impl From<OutputType> for ffi::FMOD_OUTPUTTYPE {
                fn from(value: OutputType) -> ffi::FMOD_OUTPUTTYPE {
                    match value {
                        OutputType::Autodetect => ffi::FMOD_OUTPUTTYPE_AUTODETECT,
                        OutputType::Unknown => ffi::FMOD_OUTPUTTYPE_UNKNOWN
                    }
                }
            }

            impl OutputType {
                pub fn from(value: ffi::FMOD_OUTPUTTYPE) -> Result<OutputType, Error> {
                    match value {
                        ffi::FMOD_OUTPUTTYPE_AUTODETECT => Ok(OutputType::Autodetect),
                        ffi::FMOD_OUTPUTTYPE_UNKNOWN => Ok(OutputType::Unknown),
                        _ => Err(err_enum!("FMOD_OUTPUTTYPE" , value)),
                    }
                }
            }
        }
        .to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_should_generate_enumeration_with_digits() {
        let enumeration = Enumeration {
            name: "FMOD_SPEAKERMODE".to_string(),
            enumerators: vec![
                Enumerator {
                    name: "FMOD_SPEAKERMODE_DEFAULT".to_string(),
                    value: None,
                },
                Enumerator {
                    name: "FMOD_SPEAKERMODE_5POINT1".to_string(),
                    value: None,
                },
            ],
        };
        let actual = generate_enumeration_code(&enumeration).to_string();
        let expected = quote! {
            #[derive(Debug, Clone, Copy, PartialEq)]
            pub enum SpeakerMode {
                Default,
                _5Point1
            }

            impl From<SpeakerMode> for ffi::FMOD_SPEAKERMODE {
                fn from(value: SpeakerMode) -> ffi::FMOD_SPEAKERMODE {
                    match value {
                        SpeakerMode::Default => ffi::FMOD_SPEAKERMODE_DEFAULT,
                        SpeakerMode::_5Point1 => ffi::FMOD_SPEAKERMODE_5POINT1
                    }
                }
            }

            impl SpeakerMode {
                pub fn from(value: ffi::FMOD_SPEAKERMODE) -> Result<SpeakerMode, Error> {
                    match value {
                        ffi::FMOD_SPEAKERMODE_DEFAULT => Ok(SpeakerMode::Default),
                        ffi::FMOD_SPEAKERMODE_5POINT1 => Ok(SpeakerMode::_5Point1),
                        _ => Err(err_enum!("FMOD_SPEAKERMODE" , value)),
                    }
                }
            }
        }
        .to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_should_generate_enumeration_with_mismatch_names() {
        let enumeration = Enumeration {
            name: "FMOD_STUDIO_PARAMETER_TYPE".to_string(),
            enumerators: vec![
                Enumerator {
                    name: "FMOD_STUDIO_PARAMETER_GAME_CONTROLLED".to_string(),
                    value: None,
                },
                Enumerator {
                    name: "FMOD_STUDIO_PARAMETER_AUTOMATIC_DISTANCE".to_string(),
                    value: None,
                },
            ],
        };
        let actual = generate_enumeration_code(&enumeration).to_string();
        let expected = quote! {
            #[derive(Debug, Clone, Copy, PartialEq)]
            pub enum ParameterType {
                GameControlled,
                AutomaticDistance
            }

            impl From<ParameterType> for ffi::FMOD_STUDIO_PARAMETER_TYPE {
                fn from(value: ParameterType) -> ffi::FMOD_STUDIO_PARAMETER_TYPE {
                    match value {
                        ParameterType::GameControlled => ffi::FMOD_STUDIO_PARAMETER_GAME_CONTROLLED,
                        ParameterType::AutomaticDistance => ffi::FMOD_STUDIO_PARAMETER_AUTOMATIC_DISTANCE
                    }
                }
            }

            impl ParameterType {
                pub fn from(value: ffi::FMOD_STUDIO_PARAMETER_TYPE) -> Result<ParameterType, Error> {
                    match value {
                        ffi::FMOD_STUDIO_PARAMETER_GAME_CONTROLLED => Ok(ParameterType::GameControlled),
                        ffi::FMOD_STUDIO_PARAMETER_AUTOMATIC_DISTANCE => Ok(ParameterType::AutomaticDistance),
                        _ => Err(err_enum!("FMOD_STUDIO_PARAMETER_TYPE" , value)),
                    }
                }
            }
        }
        .to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_should_generate_enumeration_with_short_enumerator() {
        let enumeration = Enumeration {
            name: "FMOD_STUDIO_LOAD_MEMORY_MODE".to_string(),
            enumerators: vec![
                Enumerator {
                    name: "FMOD_STUDIO_LOAD_MEMORY".to_string(),
                    value: None,
                },
                Enumerator {
                    name: "FMOD_STUDIO_LOAD_MEMORY_POINT".to_string(),
                    value: None,
                },
            ],
        };
        let actual = generate_enumeration_code(&enumeration).to_string();
        let expected = quote! {
            #[derive(Debug, Clone, Copy, PartialEq)]
            pub enum LoadMemoryMode {
                Memory,
                MemoryPoint
            }

            impl From<LoadMemoryMode> for ffi::FMOD_STUDIO_LOAD_MEMORY_MODE {
                fn from(value: LoadMemoryMode) -> ffi::FMOD_STUDIO_LOAD_MEMORY_MODE {
                    match value {
                        LoadMemoryMode::Memory => ffi::FMOD_STUDIO_LOAD_MEMORY,
                        LoadMemoryMode::MemoryPoint => ffi::FMOD_STUDIO_LOAD_MEMORY_POINT
                    }
                }
            }

            impl LoadMemoryMode {
                pub fn from(value: ffi::FMOD_STUDIO_LOAD_MEMORY_MODE) -> Result<LoadMemoryMode, Error> {
                    match value {
                        ffi::FMOD_STUDIO_LOAD_MEMORY => Ok(LoadMemoryMode::Memory),
                        ffi::FMOD_STUDIO_LOAD_MEMORY_POINT => Ok(LoadMemoryMode::MemoryPoint),
                        _ => Err(err_enum!("FMOD_STUDIO_LOAD_MEMORY_MODE" , value)),
                    }
                }
            }
        }
        .to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_should_generate_structure() {
        let structure = Structure {
            name: "FMOD_VECTOR".to_string(),
            fields: vec![
                Field {
                    as_const: None,
                    as_array: None,
                    field_type: FundamentalType("float".to_string()),
                    pointer: None,
                    name: "x".to_string(),
                },
                Field {
                    as_const: None,
                    as_array: None,
                    field_type: FundamentalType("float".to_string()),
                    pointer: None,
                    name: "y".to_string(),
                },
                Field {
                    as_const: None,
                    as_array: None,
                    field_type: FundamentalType("float".to_string()),
                    pointer: None,
                    name: "z".to_string(),
                },
            ],
            union: None,
        };
        let actual = generate_structure_code(&structure).unwrap().to_string();
        let expected = quote! {
            #[derive(Debug, Clone)]
            pub struct Vector {
                pub x: f32,
                pub y: f32,
                pub z: f32
            }

            impl From<ffi::FMOD_VECTOR> for Vector {
                fn from (value: ffi::FMOD_VECTOR) -> Self {
                    Self {
                        x: value.x,
                        y: value.y,
                        z: value.z
                    }
                }
            }
        }
        .to_string();
        assert_eq!(actual, expected)
    }

    #[test]
    fn test_should_generate_structure_with_keyword_field() {
        let structure = Structure {
            name: "FMOD_PLUGINLIST".to_string(),
            fields: vec![Field {
                as_const: None,
                as_array: None,
                field_type: UserType("FMOD_PLUGINTYPE".to_string()),
                pointer: None,
                name: "type".to_string(),
            }],
            union: None,
        };
        let actual = generate_structure_code(&structure).unwrap().to_string();
        let expected = quote! {
            #[derive(Debug, Clone)]
            pub struct PluginList {
                pub type_: PluginType
            }

            impl From<ffi::FMOD_PLUGINLIST> for PluginList {
                fn from (value: ffi::FMOD_PLUGINLIST) -> Self {
                    Self {
                        type_: value.type_
                    }
                }
            }
        }
        .to_string();
        assert_eq!(actual, expected)
    }
}
