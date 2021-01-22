use wasm_bindgen::JsCast;
use web_sys::{Document, Window};

#[track_caller]
pub fn window() -> Window {
    web_sys::window().expect("Can't get Window")
}

#[track_caller]
pub fn document() -> Document {
    window().document().expect("Can't get Document")
}

#[track_caller]
pub fn select_element<T: JsCast>(selector: &str) -> Option<T> {
    document()
        .query_selector(selector)
        .unwrap_or_else(|err| panic!("Can't select element by selector {:?}: {:?}", selector, err))
        .map(|element| element.dyn_into::<T>().expect("Can't cast to element"))
}

#[track_caller]
pub fn select_exist_element<T: JsCast>(selector: &str) -> T {
    select_element(selector).unwrap_or_else(|| panic!("Element not found by selector {:?}", selector))
}
