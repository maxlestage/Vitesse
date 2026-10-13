# Documentación de Vitesse

Todo lo que necesitas para crear aplicaciones web rápidas con Vitesse, el framework al estilo de Express.js para Rust: desde tu primera ruta hasta el despliegue en producción. También puedes leer estas páginas en el sitio web: https://vitesse-f16135654c3d.herokuapp.com/es/docs/

## Primeros pasos

- [Introducción](introduction.md): qué es Vitesse, su filosofía y lo que ofrece.
- [Instalación](installation.md): añadir Vitesse a un proyecto con Cargo.
- [Tu primera aplicación](first-app.md): crear y ejecutar una pequeña aplicación, paso a paso.

## Lo esencial

- [Enrutamiento](routing.md): rutas, métodos HTTP, parámetros de ruta, restricciones y parámetros opcionales, comodines, la ruta elegida y las rutas con nombre.
- [Leer la petición](requests.md): parámetros, query string, cabeceras, cookies, negociación de contenido, cuerpos (JSON, formularios) y subida de archivos.
- [Responder](responses.md): texto, JSON, HTML, códigos de estado, cabeceras, cookies, redirecciones, archivos, streaming y Server-Sent Events.
- [Plantillas HTML](templates.md): renderizar páginas HTML con minijinja (feature `templates`): plantillas base y herencia, bucles, filtros y escapado automático.
- [Middlewares](middleware.md): middlewares globales y por ruta, `next`, los middlewares incluidos (registros, CORS, autenticación, limitación de peticiones, compresión…) y las capas de tower.
- [Routers](routers.md): agrupar rutas con `Router` y montarlas bajo un prefijo, con sus propios middlewares y su fallback.
- [Estado compartido](state.md): compartir configuración, contadores o pools de conexiones entre peticiones.
- [Cookies firmadas y sesiones](sessions.md): cookies a prueba de manipulaciones y datos por visitante guardados en el servidor (feature `session`), con tu propio almacén.
- [Manejo de errores](errors.md): `vitesse::Error`, el operador `?`, respuestas de error personalizadas y pánicos.
- [Archivos estáticos](static-files.md): servir una carpeta de archivos con caché, peticiones parciales y protecciones integradas.
- [WebSocket](websocket.md): conexiones bidireccionales en tiempo real con `app.ws`, sobre HTTP/1.1 y HTTP/2, desde un servidor de eco hasta una sala de chat.

## Profundizando

- [Pruebas](testing.md): probar tu aplicación en memoria con `TestClient`, sin red.
- [Configuración del servidor](server.md): `run`, `listen` y `bind`, direcciones de escucha y sockets Unix, workers, tiempos de espera, límites y apagado.
- [HTTPS y HTTP/2](https.md): servir tú mismo HTTPS y HTTP/2 con rustls, certificados, redirecciones y h2c.
- [HTTP/3 y QUIC](http3.md): servir tu aplicación en HTTP/3 junto a HTTP/1.1, certificados y despliegue.
- [Rendimiento](performance.md): por qué Vitesse es rápido, los benchmarks y consejos de ajuste.
- [Viniendo de Express](from-express.md): la tabla de equivalencias Express → Vitesse y consejos de migración.
- [Viniendo de axum o actix-web](from-axum-actix.md): las equivalencias, funcionalidad por funcionalidad.

## Despliegue

- [Desplegar en Heroku desde el móvil](heroku-mobile.md): el botón Deploy de un solo toque y los despliegues automáticos con GitHub Actions, todo desde el teléfono.
- [Docker](docker.md): construir y ejecutar la imagen multietapa y desplegarla en plataformas de contenedores.
- [Puesta en producción](production.md): compilación release, HTTPS y proxies inversos, la dirección real del cliente, systemd, apagado ordenado, límites, seguridad y monitorización.

## Referencia

- [Chuleta de la API](cheatsheet.md): toda la API de un vistazo.
- [Preguntas frecuentes y límites](faq.md): dudas habituales y lo que Vitesse (todavía) no hace.

## Más recursos

- Referencia de la API generada a partir del código fuente: https://docs.rs/vitesse
- Código fuente, issues y ejemplos: https://github.com/maxlestage/Vitesse
- También disponible en [inglés](https://vitesse-f16135654c3d.herokuapp.com/en/docs/) y en [francés](https://vitesse-f16135654c3d.herokuapp.com/fr/docs/).
