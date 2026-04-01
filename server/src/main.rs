use maud::html;

#[rocket::get("/")]
fn index() -> maud::Markup {
    html! {
        html {
            head { title { "Toro" } }
            body {
                h1 { "Hello, world!" }
            }
        }
    }
}

#[rocket::launch]
fn rocket() -> _ {
    rocket::build().mount("/", rocket::routes![index])
}
