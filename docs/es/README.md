# Documentación de Vitesse

Todo lo que necesitas para crear aplicaciones web rápidas con Vitesse, el framework al estilo de Express.js para Rust: desde tu primera ruta hasta el despliegue en producción. También puedes leer estas páginas en el sitio web: https://maxlestage.github.io/Vitesse/#/docs

## Primeros pasos

- [Introducción](introduction.md): qué es Vitesse, su filosofía y lo que ofrece.
- [Instalación](installation.md): añadir Vitesse a un proyecto con Cargo.
- [Tu primera aplicación](first-app.md): crear y ejecutar una pequeña aplicación, paso a paso.

## Lo esencial

- [Enrutamiento](routing.md): rutas, métodos HTTP, parámetros de ruta y comodines.
- [Leer la petición](requests.md): parámetros, query string, cabeceras, cookies y cuerpos (JSON, formularios).
- [Responder](responses.md): texto, JSON, HTML, códigos de estado, cabeceras, cookies, redirecciones y archivos.
- [Middlewares](middleware.md): middlewares globales y por ruta, `next` y los middlewares incluidos.
- [Routers](routers.md): agrupar rutas con `Router` y montarlas bajo un prefijo.
- [Estado compartido](state.md): compartir configuración, contadores o pools de conexiones entre peticiones.
- [Manejo de errores](errors.md): `vitesse::Error`, el operador `?`, respuestas de error personalizadas y pánicos.
- [Archivos estáticos](static-files.md): servir una carpeta de archivos con caché, peticiones parciales y protecciones integradas.

## Profundizando

- [Pruebas](testing.md): probar tu aplicación en memoria con `TestClient`, sin red.
- [Configuración del servidor](server.md): `run`, `listen` y `bind`, direcciones de escucha, workers, límites y apagado.
- [Rendimiento](performance.md): por qué Vitesse es rápido, los benchmarks y consejos de ajuste.
- [Viniendo de Express](from-express.md): la tabla de equivalencias Express → Vitesse y consejos de migración.

## Despliegue

- [Desplegar en Heroku desde el móvil](heroku-mobile.md): el botón Deploy de un solo toque y los despliegues automáticos con GitHub Actions, todo desde el teléfono.
- [Docker](docker.md): construir y ejecutar la imagen multietapa y desplegarla en plataformas de contenedores.
- [Puesta en producción](production.md): compilación release, proxy inverso, systemd, apagado ordenado, límites, seguridad y monitorización.

## Referencia

- [Chuleta de la API](cheatsheet.md): toda la API de un vistazo.
- [Preguntas frecuentes y límites](faq.md): dudas habituales y lo que Vitesse (todavía) no hace.

## Más recursos

- Referencia de la API generada a partir del código fuente: https://docs.rs/vitesse
- Código fuente, issues y ejemplos: https://github.com/maxlestage/Vitesse
- También disponible en [inglés](https://github.com/maxlestage/Vitesse/blob/master/docs/en/README.md) y en [francés](https://github.com/maxlestage/Vitesse/blob/master/docs/fr/README.md).
