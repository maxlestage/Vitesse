// Serveur Drogon de référence, mêmes routes que `examples/bench.rs`.
// Variables d'environnement : WORKERS (threads), PORT (3003 par défaut).
#include <drogon/drogon.h>

#include <cstdlib>

using namespace drogon;

int main() {
    const char *w = std::getenv("WORKERS");
    const char *p = std::getenv("PORT");
    const size_t workers = w ? std::strtoul(w, nullptr, 10) : 2;
    const uint16_t port = p ? static_cast<uint16_t>(std::strtoul(p, nullptr, 10)) : 3003;

    app().registerHandler(
        "/",
        [](const HttpRequestPtr &, std::function<void(const HttpResponsePtr &)> &&callback) {
            auto resp = HttpResponse::newHttpResponse();
            resp->setBody("Hello, World!");
            resp->setContentTypeCode(CT_TEXT_PLAIN);
            callback(resp);
        },
        {Get});

    app().registerHandler(
        "/json",
        [](const HttpRequestPtr &, std::function<void(const HttpResponsePtr &)> &&callback) {
            Json::Value json;
            json["message"] = "Hello, World!";
            callback(HttpResponse::newHttpJsonResponse(json));
        },
        {Get});

    app().registerHandler(
        "/users/{id}",
        [](const HttpRequestPtr &, std::function<void(const HttpResponsePtr &)> &&callback,
           const std::string &id) {
            Json::Value json;
            json["id"] = id;
            json["name"] = "Ada";
            callback(HttpResponse::newHttpJsonResponse(json));
        },
        {Get});

    app().registerHandler(
        "/echo",
        [](const HttpRequestPtr &req, std::function<void(const HttpResponsePtr &)> &&callback) {
            auto json = req->getJsonObject();
            if (!json) {
                auto resp = HttpResponse::newHttpResponse();
                resp->setStatusCode(k400BadRequest);
                callback(resp);
                return;
            }
            callback(HttpResponse::newHttpJsonResponse(*json));
        },
        {Post});

    app()
        .setLogLevel(trantor::Logger::kWarn)
        .setThreadNum(workers)
        .addListener("0.0.0.0", port)
        .run();
}
