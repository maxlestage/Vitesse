# Validation

Deserializing a body with serde checks its *shape*: `{"age": "ten"}` is not a `u8`, and `req.json()` answers `400`. Validation checks its *content*: an email address that looks like one, a name that isn't empty, an age above 13. Vitesse's `validate` module does it the way Laravel's `$request->validate()` or Fastify's schemas do: every rule that fails is reported, field by field, in a `422 Unprocessable Entity` the client can show next to its form.

## Validating a body

Implement `Validate` for the type you receive, then read it with `req.valid_json()` instead of `req.json()`:

```rust
use serde::Deserialize;
use vitesse::prelude::*;
use vitesse::validate::{Validate, Validator};

#[derive(Deserialize)]
struct SignUp {
    email: String,
    name: String,
    age: u8,
    website: Option<String>,
}

impl Validate for SignUp {
    fn validate(&self, v: &mut Validator) {
        v.field("email", &self.email).email();
        v.field("name", &self.name).not_empty().max_length(50);
        v.field("age", &self.age).min(13);
        v.optional("website", &self.website).url();
    }
}

let mut app = App::new();
app.post("/users", |req: Request| async move {
    let user: SignUp = req.valid_json().await?;
    Ok::<_, Error>((201, Json(json!({ "email": user.email }))))
});
```

`valid_json` deserializes the body, then runs the rules:

```text
POST /users {"email":"ada@example.com","name":"Ada","age":36}
→ 201 {"email":"ada@example.com"}

POST /users {"email":"nope","name":"","age":9}
→ 422 {"error":"Validation failed","details":{"age":["must be at least 13"],"email":["must be a valid email address"],"name":["must not be empty"]}}

POST /users {"email":
→ 400 {"error":"invalid JSON: EOF while parsing a value at line 1 column 9"}
```

The same exists for forms and query strings:

| Method | Reads | Then |
|---|---|---|
| `req.valid_json::<T>().await?` | A JSON body, like `req.json()` | Checks it: `422` if a rule fails |
| `req.valid_form::<T>().await?` | A URL-encoded form, like `req.form()` | Same |
| `req.valid_query::<T>()?` | The query string, like `req.query_as()` | Same |
| `vitesse::validate::validate(&value)?` | A value you already have | Same |

Validation only runs where you call it: routes that don't use it don't pay for it.

## The rules

A field starts with `v.field(name, &value)` and chains its rules. A field stops at its **first** failing rule, so the client gets one message per problem, not "is empty" *and* "is too short".

| Rule | For | Fails when |
|---|---|---|
| `.not_empty()` | strings, `Vec`, slices, maps, sets | the value is empty |
| `.min_length(n)`, `.max_length(n)` | the same | it has fewer / more than `n` characters (Unicode characters, not bytes) or items |
| `.min(x)`, `.max(x)` | numbers, and anything `PartialOrd + Display` | the value is below / above `x` |
| `.between(a, b)` | the same | the value is outside `a..=b` |
| `.one_of(&["S", "M", "L"])` | anything comparable with the choices | the value isn't one of them |
| `.email()` | strings | it doesn't look like `name@domain.tld` |
| `.url()` | strings | it isn't an `http://` or `https://` URL with a host |
| `.alphanumeric()` | strings | it contains something other than ASCII letters and digits |
| `.check(\|v\| …, "message")` | anything | your function returns `false` |

### Optional and required fields

For an `Option`, `v.optional(name, &value)` skips the rules when the value is `None`, and `v.required(name, &value)` reports "is required" instead:

```rust
v.optional("website", &self.website).url();       // absent is fine
v.required("password", &self.password).min_length(12); // absent is an error
```

> [!TIP]
> serde already rejects a missing non-`Option` field with a `400` (`missing field 'email'`). Declare the field as an `Option` and use `v.required(…)` when you want a missing field reported in the `422`, with the others.

### Your own rules and messages

`.check` adds a rule of your own, and `.message` replaces the message of the rule just before it, if it failed: that's also how you translate them.

```rust
impl Validate for Account {
    fn validate(&self, v: &mut Validator) {
        v.field("username", &self.username)
            .min_length(3)
            .message("au moins 3 caractères")
            .check(|u| !u.contains(' '), "must not contain spaces");
        if self.password != self.password_confirmation {
            v.error("password_confirmation", "does not match the password");
        }
    }
}
```

`v.error(name, message)` reports a problem directly: rules across several fields, or a check you made beforehand (a username already taken in your database, for instance).

### Nested values and lists

`v.nested` validates a value that implements `Validate` itself, and `v.each` every item of a list. Their fields are reported with a path:

```rust
impl Validate for Order {
    fn validate(&self, v: &mut Validator) {
        v.nested("address", &self.address); // "address.city"
        v.field("items", &self.items).not_empty();
        v.each("items", &self.items);       // "items[2].quantity"
    }
}
```

## The response

A failed validation is a regular `vitesse::Error`: a `422` whose message is `Validation failed`, and whose `details` are the messages of each field, sorted by name. It goes through `app.on_error` like any other error, which can read `err.details()` to render it differently: an HTML form with its errors, or the `application/problem+json` format, for instance. See [Error handling](errors.md#structured-details-with_details).

To validate without going through a request (a value read from a file, say), call `validate::validate(&value)`, or build a `Validator` yourself: `is_valid()`, `errors()` and `into_result()` give you the result.

## Typed route parameters

Path parameters are validated with types and constraints rather than rules: a constraint in the pattern (`r"/users/:id(\d+)"`) keeps the route from matching, and `req.param_as::<u64>("id")?` or `req.params_as::<T>()?` answer `400` when a value doesn't convert. See [Typed parameters](routing.md#typed-parameters).
