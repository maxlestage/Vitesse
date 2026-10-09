-- POST d'un petit document JSON.
wrk.method = "POST"
wrk.body = '{"name":"Ada","langages":["rust","js"],"age":36}'
wrk.headers["Content-Type"] = "application/json"
