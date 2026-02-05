wit_bindgen::generate!({
    world: "hello",
    exports: {
        world: Component,
    },
});

struct Component;

impl Guest for Component {
    fn greet(name: String) -> String {
        format!("Hello, {}!", name)
    }
}
