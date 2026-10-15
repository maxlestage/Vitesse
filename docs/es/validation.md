# Validación

Deserializar un cuerpo con serde comprueba su *forma*: `{"age": "diez"}` no es un `u8`, y `req.json()` responde `400`. La validación comprueba su *contenido*: un correo electrónico que lo parezca, un nombre que no esté vacío, una edad de al menos 13 años. El módulo `validate` de Vitesse lo hace como el `$request->validate()` de Laravel o los esquemas de Fastify: cada regla que falla se informa, campo por campo, en una `422 Unprocessable Entity` que el cliente puede mostrar junto a su formulario.

## Validar un cuerpo

Implementa `Validate` para el tipo que recibes y léelo con `req.valid_json()` en lugar de `req.json()`:

```rust
use serde::Deserialize;
use vitesse::prelude::*;
use vitesse::validate::{Validate, Validator};

#[derive(Deserialize)]
struct Registro {
    email: String,
    nombre: String,
    edad: u8,
    web: Option<String>,
}

impl Validate for Registro {
    fn validate(&self, v: &mut Validator) {
        v.field("email", &self.email).email();
        v.field("nombre", &self.nombre).not_empty().max_length(50);
        v.field("edad", &self.edad).min(13);
        v.optional("web", &self.web).url();
    }
}

let mut app = App::new();
app.post("/usuarios", |req: Request| async move {
    let registro: Registro = req.valid_json().await?;
    Ok::<_, Error>((201, Json(json!({ "email": registro.email }))))
});
```

`valid_json` deserializa el cuerpo y luego aplica las reglas:

```text
POST /usuarios {"email":"ada@example.com","nombre":"Ada","edad":36}
→ 201 {"email":"ada@example.com"}

POST /usuarios {"email":"no","nombre":"","edad":9}
→ 422 {"error":"Validation failed","details":{"edad":["must be at least 13"],"email":["must be a valid email address"],"nombre":["must not be empty"]}}

POST /usuarios {"email":
→ 400 {"error":"invalid JSON: EOF while parsing a value at line 1 column 9"}
```

Lo mismo existe para formularios y query strings:

| Método | Lee | Después |
|---|---|---|
| `req.valid_json::<T>().await?` | Un cuerpo JSON, como `req.json()` | Lo comprueba: `422` si falla una regla |
| `req.valid_form::<T>().await?` | Un formulario URL-encoded, como `req.form()` | Igual |
| `req.valid_query::<T>()?` | La query string, como `req.query_as()` | Igual |
| `vitesse::validate::validate(&valor)?` | Un valor que ya tienes | Igual |

La validación solo se ejecuta donde la llamas: las rutas que no la usan no pagan nada.

## Las reglas

Un campo empieza con `v.field(nombre, &valor)` y encadena sus reglas. Se detiene en su **primera** regla que falla: el cliente recibe un mensaje por problema, y no «está vacío» *y* «es demasiado corto».

| Regla | Para | Falla si |
|---|---|---|
| `.not_empty()` | cadenas, `Vec`, slices, mapas, conjuntos | el valor está vacío |
| `.min_length(n)`, `.max_length(n)` | los mismos | tiene menos / más de `n` caracteres (caracteres Unicode, no bytes) o elementos |
| `.min(x)`, `.max(x)` | números, y todo lo que sea `PartialOrd + Display` | el valor es menor / mayor que `x` |
| `.between(a, b)` | los mismos | el valor está fuera de `a..=b` |
| `.one_of(&["S", "M", "L"])` | todo lo que se compare con las opciones | el valor no es ninguna de ellas |
| `.email()` | cadenas | no parece `nombre@dominio.tld` |
| `.url()` | cadenas | no es una URL `http://` o `https://` con un host |
| `.alphanumeric()` | cadenas | contiene algo que no son letras y dígitos ASCII |
| `.check(\|v\| …, "mensaje")` | todo | tu función devuelve `false` |

### Campos opcionales y obligatorios

Para una `Option`, `v.optional(nombre, &valor)` omite las reglas cuando el valor es `None`, y `v.required(nombre, &valor)` informa entonces «is required»:

```rust
v.optional("web", &self.web).url();                     // ausente: aceptado
v.required("contrasena", &self.contrasena).min_length(12); // ausente: error
```

> [!TIP]
> serde ya rechaza un campo ausente que no sea una `Option`, con un `400` (`missing field 'email'`). Declara el campo como `Option` y usa `v.required(…)` si quieres que el campo que falta aparezca en el `422`, junto a los demás.

### Tus propias reglas y mensajes

`.check` añade una regla tuya, y `.message` reemplaza el mensaje de la regla justo anterior, si ha fallado: así también se traducen.

```rust
impl Validate for Cuenta {
    fn validate(&self, v: &mut Validator) {
        v.field("usuario", &self.usuario)
            .min_length(3)
            .message("al menos 3 caracteres")
            .check(|u| !u.contains(' '), "sin espacios");
        if self.contrasena != self.confirmacion {
            v.error("confirmacion", "no coincide con la contraseña");
        }
    }
}
```

`v.error(nombre, mensaje)` informa un problema directamente: una regla sobre varios campos, o una comprobación hecha antes (un nombre de usuario ya ocupado en tu base de datos, por ejemplo).

### Valores anidados y listas

`v.nested` valida un valor que implementa a su vez `Validate`, y `v.each` cada elemento de una lista. Sus campos se informan con una ruta:

```rust
impl Validate for Pedido {
    fn validate(&self, v: &mut Validator) {
        v.nested("direccion", &self.direccion); // "direccion.ciudad"
        v.field("articulos", &self.articulos).not_empty();
        v.each("articulos", &self.articulos);   // "articulos[2].cantidad"
    }
}
```

## La respuesta

Una validación que falla es un `vitesse::Error` normal: un `422` cuyo mensaje es `Validation failed` y cuyos `details` son los mensajes de cada campo, ordenados por nombre. Pasa por `app.on_error` como cualquier error, que puede leer `err.details()` para presentarlo de otra forma: un formulario HTML con sus errores, o el formato `application/problem+json`, por ejemplo. Consulta [Manejo de errores](errors.md#detalles-estructurados-with_details).

Para validar sin pasar por una petición (un valor leído de un archivo, por ejemplo), llama a `validate::validate(&valor)`, o construye tú mismo un `Validator`: `is_valid()`, `errors()` e `into_result()` dan el resultado.

## Parámetros de ruta tipados

Los parámetros de ruta se validan por su tipo y sus restricciones más que con reglas: una restricción en el patrón (`r"/users/:id(\d+)"`) impide que la ruta coincida, y `req.param_as::<u64>("id")?` o `req.params_as::<T>()?` responden `400` cuando un valor no se convierte. Consulta [Parámetros tipados](routing.md#parámetros-tipados).
