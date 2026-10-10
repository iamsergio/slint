// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

//! This pass turns instances of `native component` declarations into plain properties and
//! callbacks of the enclosing component, and records them in [`Component::native_components`].

use crate::diagnostics::BuildDiagnostics;
use crate::expression_tree::{Callable, Expression, NamedReference};
use crate::langtype::{ElementType, Type};
use crate::object_tree::*;
use crate::typeregister::TypeRegister;
use std::rc::Rc;

pub fn lower_native_components(
    component: &Rc<Component>,
    type_register: &TypeRegister,
    supported: bool,
    diag: &mut BuildDiagnostics,
) {
    let mut instances = Vec::new();
    recurse_elem_including_sub_components_no_borrow(
        component,
        &None,
        &mut |elem, parent_element: &Option<ElementRc>| {
            let is_native_instance = matches!(&elem.borrow().base_type,
                ElementType::Component(c) if c.is_native());
            if is_native_instance {
                instances.push((elem.clone(), parent_element.clone()));
            }
            Some(elem.clone())
        },
    );
    if instances.is_empty() {
        return;
    }

    let mut lowered = false;
    for (elem, parent_element) in instances {
        lowered |= lower_instance(&elem, parent_element.as_ref(), type_register, supported, diag);
    }
    if !lowered {
        return;
    }

    visit_all_expressions(component, |e, _| {
        e.visit_recursive_mut(&mut |e| {
            if let Expression::FunctionCall { function, .. } = e
                && let Callable::Function(nr) = function
                && matches!(nr.ty(), Type::Callback(_))
            {
                *function = Callable::Callback(nr.clone());
            }
        });
    });
}

fn lower_instance(
    elem: &ElementRc,
    parent_element: Option<&ElementRc>,
    type_register: &TypeRegister,
    supported: bool,
    diag: &mut BuildDiagnostics,
) -> bool {
    let ElementType::Component(declaration) = elem.borrow().base_type.clone() else {
        return false;
    };
    let parent_component = elem.borrow().enclosing_component.upgrade().unwrap();

    if !supported {
        diag.push_error(
            "'native component' is only supported when generating C++".into(),
            &*elem.borrow(),
        );
        return false;
    }
    let parent_element = match parent_element {
        Some(parent) if !Rc::ptr_eq(&parent_component.root_element, elem) => parent,
        _ => {
            diag.push_error(
                format!(
                    "'{}' can't be repeated, conditional, or inherited from; place it inside another element",
                    declaration.id
                ),
                &*elem.borrow(),
            );
            return false;
        }
    };
    if !elem.borrow().children.is_empty() {
        diag.push_error("A native component can't have children".into(), &*elem.borrow());
        return false;
    }

    let mut members = Vec::new();
    {
        let declaration_root = declaration.root_element.borrow();
        let mut elem_mut = elem.borrow_mut();
        for (name, decl) in &declaration_root.property_declarations {
            let mut decl = decl.clone();
            if let Type::Function(f) = &decl.property_type {
                decl.property_type = Type::Callback(f.clone());
            }
            elem_mut.property_declarations.insert(name.clone(), decl);
        }
        elem_mut.base_type = type_register.empty_type();
    }
    for (name, decl) in &declaration.root_element.borrow().property_declarations {
        let nr = NamedReference::new(elem, name.clone());
        let is_function = matches!(decl.property_type, Type::Function(_));
        let is_output_property =
            !matches!(decl.property_type, Type::Function(_) | Type::Callback(_))
                && decl.visibility != PropertyVisibility::Input;
        if is_function || is_output_property {
            nr.mark_as_set();
        }
        members.push((name.clone(), nr));
    }

    move_to_optimized_elements(elem, parent_element);
    parent_component.native_components.borrow_mut().push(NativeComponentInstance {
        declaration,
        element: Rc::downgrade(elem),
        members,
    });
    true
}
