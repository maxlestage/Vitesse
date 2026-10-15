# OpenAPI y Swagger UI

Vitesse genera el documento [OpenAPI 3.1](https://spec.openapis.org/oas/v3.1.0) de tu aplicación a partir de sus rutas, como FastAPI o `@fastify/swagger`, y puede servir una página Swagger UI para leer y probar la API en un navegador. Los generadores de clientes, Postman, las pasarelas de API y las herramientas de documentación leen ese documento.

## Servir el documento

Una sola llamada, en cualquier punto de la configuración de la aplicación:

```rust
use vitesse::prelude::*;
use vitesse::openapi::OpenApi;

let mut app = App::new();
app.get("/users/:id", mostrar);
app.post("/users", crear);

app.openapi(
    "/openapi.json",
    OpenApi::new("API de usuarios", "1.0.0").swagger_ui("/docs"),
);
app.run(3000)
```

`GET /openapi.json` responde ahora el documento, y `GET /docs` la página Swagger UI (carga Swagger UI desde el CDN jsDelivr). Se listan todas las rutas, incluidos los routers montados y los grupos, con los parámetros de su ruta: `/users/:id` se convierte en `/users/{id}`, y una restricción como `r"/users/:id(\d+)"` se convierte en el `pattern` del esquema del parámetro. Los parámetros opcionales dan una ruta por variante (`/archive` y `/archive/{year}`).

El documento se construye una vez, al arrancar, y se sirve desde memoria: no cuesta nada por petición, y nada en absoluto si no llamas a `app.openapi`.

## Describir una ruta

`.doc(…)` sigue a una ruta, como `.name(…)`, y la describe con una `Operation`:

```rust
use vitesse::openapi::Operation;

app.get(r"/users/:id(\d+)", mostrar)
    .name("user")
    .doc(
        Operation::new()
            .summary("Leer un usuario")
            .tag("usuarios")
            .path_param("id", "El identificador", json!({ "type": "integer" }))
            .response_json(200, "El usuario", json!({ "$ref": "#/components/schemas/User" }))
            .response(404, "Usuario desconocido"),
    );

app.post("/users", crear).doc(
    Operation::new()
        .summary("Crear un usuario")
        .tag("usuarios")
        .body(json!({
            "type": "object",
            "required": ["email"],
            "properties": { "email": { "type": "string", "format": "email" } }
        }))
        .response(201, "Creado")
        .response(422, "Campos no válidos"),
);
```

Los esquemas son JSON Schema normal, escrito con `json!`: no hay macros que aprender, y lo que escribes es lo que leen los clientes.

| Método de `Operation` | Define |
|---|---|
| `.summary(…)`, `.description(…)` | El resumen y la descripción (Markdown) |
| `.tag(…)` | Una etiqueta, para agrupar operaciones (se admiten varias) |
| `.operation_id(…)` | El identificador que los generadores de clientes usan como nombre de función |
| `.path_param(nombre, descripción, esquema)` | Describe un parámetro de ruta (si no, se lista como cadena) |
| `.query(nombre, descripción, esquema)`, `.query_required(…)` | Un parámetro de query string |
| `.header(nombre, descripción, esquema, obligatorio)` | Una cabecera de la petición |
| `.body(esquema)`, `.body_with(tipo, esquema)` | Un cuerpo obligatorio, JSON o de otro tipo |
| `.response(estado, descripción)`, `.response_json(estado, descripción, esquema)` | Una respuesta posible (`200 OK` si no se da ninguna) |
| `.security(esquema)`, `.public()` | Exige un esquema de seguridad, o ninguno |
| `.deprecated()` | Marca la operación como obsoleta |
| `.hidden()` | Saca la ruta del documento |
| `.extend(json!({ … }))` | Fusiona cualquier otra cosa (`externalDocs`, `callbacks`…) |

`.doc` funciona en `App` y en `Router`: las descripciones de un router siguen a sus rutas cuando se monta.

## Describir la API

`OpenApi` contiene lo que vale para toda la API:

```rust
let spec = OpenApi::new("API de usuarios", "1.0.0")
    .description("Todo sobre los usuarios.")
    .server("https://api.example.com")
    .tag("usuarios", "Cuentas y perfiles")
    .schema("User", json!({
        "type": "object",
        "properties": { "id": { "type": "integer" }, "email": { "type": "string" } }
    }))
    .security_scheme("bearer", json!({ "type": "http", "scheme": "bearer" }))
    .security("bearer") // todas las operaciones, salvo `.public()`
    .extend(json!({ "info": { "contact": { "email": "api@example.com" } } }))
    .swagger_ui("/docs");
app.openapi("/openapi.json", spec);
```

Los esquemas compartidos se referencian con `json!({ "$ref": "#/components/schemas/User" })`.

## Qué se lista

- Todas las rutas con un método: las de la aplicación, sus routers y sus grupos.
- No se listan sin `.doc(…)`: las rutas para todos los métodos (`app.all`), las rutas `HEAD` y `OPTIONS`, y las rutas con comodín como los archivos estáticos. Una ruta con `.doc(Operation::new().hidden())` nunca se lista.
- Ni el propio documento ni la página Swagger UI.

## Exportar el documento

`app.openapi_document(&spec)` devuelve el documento de las rutas añadidas hasta ese momento como un `serde_json::Value`, sin servirlo: para escribirlo en un archivo al compilar, o comprobarlo en un test.

```rust
let doc = app.openapi_document(&OpenApi::new("API de usuarios", "1.0.0"));
std::fs::write("openapi.json", serde_json::to_vec_pretty(&doc)?)?;
```

En producción, quizá quieras mantener privados el documento y la página Swagger UI: protege sus rutas con un [hook `on_request`](middleware.md#hooks-del-ciclo-de-vida) o un middleware global que compruebe las credenciales, o llama a `app.openapi` solo en las compilaciones de desarrollo.
