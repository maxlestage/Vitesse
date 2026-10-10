# Deploy to Heroku from your phone

You don't need a computer to put a Vitesse app online. This guide takes you through every step from your phone's web browser: first a one-tap deployment with the Deploy button, then automatic redeployment on every commit with GitHub Actions.

> [!NOTE]
> Every step below works in a mobile web browser (Safari, Chrome, Firefox…). Heroku and GitHub rearrange their menus from time to time: if a label is slightly different from what you see, look for the closest equivalent. On github.com, if a menu or tab seems to be missing, switch your browser to its "Desktop site" mode (usually in the browser's menu).

## Before you start

You need:

- **A Heroku account**: sign up at https://signup.heroku.com. Heroku no longer has a free plan. The cheapest options are **Eco** dynos (about $5/month) and **Basic** dynos (about $7/month), but prices change: check https://www.heroku.com/pricing. Heroku asks for a **payment method** before you can create an app.
- **A GitHub account**, only for sections 2 and 3 (your own copy of the code and automatic deployments).

### How it works

The repository already contains everything Heroku needs, so nothing has to be compiled on your phone:

| File | Role |
|---|---|
| `app.json` | Describes the app for the Deploy button (Docker-based "container" stack). |
| `heroku.yml` | Tells Heroku to build the image from the `Dockerfile`. |
| `Dockerfile` | Compiles an example (`demo` by default) in release mode and packages it in a small image. |
| `examples/demo.rs` | The demo app: a home page, `/health` and a small JSON API. |
| `.github/workflows/heroku.yml` | The **Deploy to Heroku** workflow, for automatic deployments. |

Heroku builds the Docker image on its own servers, then starts it and tells the app which port to listen on.

## 1. The fastest way: the Deploy button

### Deploy the demo app

[![Deploy to Heroku](https://www.herokucdn.com/deploy/button.svg)](https://www.heroku.com/deploy?template=https://github.com/maxlestage/Vitesse)

1. Tap the button above (it is also on the project's README). Log in to Heroku if asked.
2. Heroku shows a form to create a new app:
   - **App name**: for example `my-vitesse-demo`. Names are unique across all of Heroku and use lowercase letters, digits and dashes. You can also leave it empty and let Heroku pick one.
   - **Choose a region**: United States or Europe.
3. Tap **Deploy app**. Heroku fetches the code and builds the Docker image; you can follow the build log on the page. Count a few minutes: the Rust code is compiled with every optimisation turned on.
4. When the build is done, tap **View** to open your app. You should see "⚡ Vitesse is running." The exact address (`https://….herokuapp.com`) is shown in the app's **Settings** tab on Heroku.

Then try these addresses, by adding them after your app's address in the browser bar:

| Path | Answer |
|---|---|
| `/health` | `ok` |
| `/api/hello/Ada` | `{"message":"Hello, Ada!"}` |
| `/api/todos` | `[]` (the to-do list, empty for now) |

> [!NOTE]
> The demo keeps its to-do list in memory: it is wiped every time the app restarts (new deployment, settings change, and the automatic restart Heroku performs about once a day). For data that must survive, use a database.

### Deploy your own copy (fork)

To change the code, you need your own copy of the repository on GitHub:

1. Open https://github.com/maxlestage/Vitesse, tap **Fork**, then **Create fork**.
2. Open the Deploy link with your GitHub username instead of `maxlestage`, by typing it in the address bar:

   ```text
   https://www.heroku.com/deploy?template=https://github.com/<you>/Vitesse
   ```

   You can also edit the button in your fork's `README.md` so it points to your fork (section 3 explains how to edit a file from your phone).
3. Follow the same steps as above.

> [!IMPORTANT]
> The button deploys **once**. When you change your fork later, the Heroku app is not updated automatically. To redeploy on every commit, set up the workflow in the next section, and reuse the same app name: the workflow can update the app you have just created.

## 2. Continuous deployment with GitHub Actions

Goal: every commit to the `master` branch of your fork rebuilds and redeploys the app, without you doing anything. The repository includes a workflow called **Deploy to Heroku** ([`.github/workflows/heroku.yml`](https://github.com/maxlestage/Vitesse/blob/master/.github/workflows/heroku.yml)) that talks directly to the Heroku Platform API: no Heroku CLI and no computer needed.

On each run, it:

1. checks that it is configured (if not, it stops without failing on a push);
2. creates the Heroku app if it does not exist yet (with the Docker "container" stack);
3. sends your code to Heroku, which builds the image from `heroku.yml` and releases it; the build log is streamed live in GitHub;
4. writes the app's address in the run summary.

You need your own fork (see "Deploy your own copy" above).

### Step 1: get a Heroku API token

1. In your phone's browser, go to https://dashboard.heroku.com and log in.
2. Open **Account settings** (from the menu or your avatar, at the top right).
3. Pick one of these two options:
   - **Quick**: in the **API Key** section, tap **Reveal** and copy the key.
   - **Recommended**: open the **Applications** tab and, under **Authorizations**, tap **Create authorization**. Give it a description such as "GitHub Actions" (you can usually leave the expiry empty for a long-lived token), then copy the token. A dedicated token can be revoked on its own, without affecting anything else.

> [!WARNING]
> This token acts on your behalf on your whole Heroku account. Never paste it into a file, a commit, an issue or a message: only into GitHub's secrets, as described in the next step.

### Step 2: add the secret and the variable on GitHub

1. On github.com, open **your fork** and tap **Settings** (in the repository's tab bar; scroll it sideways, or switch to "Desktop site" if you don't see it).
2. In the menu, open **Secrets and variables**, then **Actions**.
3. On the **Secrets** tab, tap **New repository secret**:
   - **Name**: `HEROKU_API_KEY`
   - **Secret**: paste the token
   - tap **Add secret**.
4. Open the **Variables** tab and tap **New repository variable**:
   - **Name**: `HEROKU_APP_NAME`
   - **Value**: the name of your app (the one created with the button, or a new name: the workflow creates the app if needed)
   - tap **Add variable**.
5. Optional: add a variable `HEROKU_REGION` with the value `eu` to create the app in Europe. It is only used when the workflow creates the app.

| Name | Kind | Required | Value |
|---|---|---|---|
| `HEROKU_API_KEY` | Secret | Yes | Your Heroku token |
| `HEROKU_APP_NAME` | Variable | Yes | For example `my-vitesse-demo` |
| `HEROKU_REGION` | Variable | No | `us` (default) or `eu` |

### Step 3: enable Actions on your fork

GitHub disables workflows on a freshly created fork. Open the **Actions** tab of your fork: if a message says that workflows aren't being run on this forked repository, tap the button that enables them.

### Step 4: run the first deployment

1. In the **Actions** tab, choose **Deploy to Heroku** in the list of workflows.
2. Tap **Run workflow**, keep the `master` branch (the "Heroku app name" field is optional: it overrides `HEROKU_APP_NAME` for this run only), then confirm with the green **Run workflow** button.
3. Tap the run that appears to follow it. The first build takes a few minutes.
4. When it turns green, the app's address is shown in the run summary.

From now on, **every commit to `master` that touches the code** redeploys the app: `src/`, `examples/`, `Cargo.toml`, `Cargo.lock`, `Dockerfile`, `heroku.yml` or the workflow itself. Changing the README or the docs does not trigger a deployment.

> [!NOTE]
> As long as the secret or the variable is missing, pushes simply skip the deployment (the run shows a notice, not a failure). A manual run, on the other hand, fails with a message saying what is missing.

## 3. Customise your app

### Edit the code from your phone

1. On github.com, in your fork, open `examples/demo.rs`.
2. Tap the **pencil** icon (✏️, "Edit this file").
3. Change something, for example the greeting of the `/api/hello/:name` route:

   ```rust
   app.get("/api/hello/:name", |req: Request| async move {
       let name = req.param("name").unwrap_or("world");
       Json(json!({ "message": format!("Howdy, {name}!") }))
   });
   ```

4. Tap **Commit changes…**, write a short message, keep "Commit directly to the `master` branch" and confirm with **Commit changes**.
5. The workflow starts by itself (you can watch it in the **Actions** tab). A few minutes later, the new version is online.

Adding a route is just as easy. Insert this line next to the other routes:

```rust
app.get("/api/ping", |_| async { json!({ "pong": true }) });
```

See [Routing](routing.md) and [Sending responses](responses.md) for everything a route can do.

> [!TIP]
> A typo in the Rust code makes the build fail, and the previous version simply stays online: Heroku only releases builds that succeed. Open the failed run in the **Actions** tab to read the compiler's message (it gives the file, line and column), fix the code and commit again.

### Use your own example

You can leave `demo.rs` alone and write your app in a new file:

1. In your fork, open the `examples` folder, tap **Add file** → **Create new file** and name it, for example, `my_app.rs`.
2. Paste a minimal app:

   ```rust
   use vitesse::prelude::*;

   fn main() -> std::io::Result<()> {
       let mut app = App::new();
       app.middleware(middleware::logger());

       app.get("/", |_| async { "Hello from my phone!" });
       app.get("/health", |_| async { "ok" });

       // Heroku chooses the port: always listen on $PORT.
       let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
       app.run(port)
   }
   ```

3. Commit the file.
4. Open the `Dockerfile`, tap the pencil and replace the line `ARG EXAMPLE=demo` with `ARG EXAMPLE=my_app` (the file name without `.rs`). Commit: the workflow builds and deploys your example.

The `EXAMPLE` build argument selects which file of `examples/` gets compiled; editing its default value in the `Dockerfile` is what makes Heroku use your example. Your code can use the crates already declared in `Cargo.toml`: `vitesse`, `serde` (with `derive`) and `tokio`.

> [!IMPORTANT]
> Your app must listen on **`0.0.0.0:$PORT`**. Heroku picks the port when the app starts and passes it in the `PORT` environment variable. `app.run(port)` with a bare port number (or a string that only contains digits) already listens on every interface. Never hard-code `3000` and never listen on `127.0.0.1` only, otherwise Heroku can't reach your app.

> [!TIP]
> Need another crate? Open `Cargo.toml` with the pencil icon, add a line under `[dev-dependencies]` (the section examples can use without changing the library's own dependencies), for example `chrono = "0.4"`, and commit. The build resolves the new crate by itself, while the others keep the versions pinned in `Cargo.lock`. Later, committing an updated `Cargo.lock` from a computer (any `cargo build` refreshes it) is still good practice for reproducible builds.

### Settings: environment variables

On Heroku, open your app, then **Settings** → **Reveal Config Vars**. Each config var becomes an environment variable, which your code reads with `std::env::var("NAME")`. Saving a change restarts the app. Don't define `PORT` yourself: Heroku sets it.

## 4. Day-to-day operations

### Logs

Heroku dashboard → your app → **More** (top right) → **View logs**. You'll see Heroku's own messages (start, stop, crashes, one line per request from its router) and everything your app prints. With `middleware::logger()`, the app writes one line per request, such as `GET /api/todos 200 0.084 ms`.

The **Activity** tab lists builds and releases, with a link to each build log.

### Restart, scale, stop

- **Restart**: **More** → **Restart all dynos**.
- **Dyno type**: in the **Resources** tab, edit the `web` line (pencil icon) to change the type (Eco, Basic, Standard…); the price changes accordingly. An Eco dyno goes to sleep after about 30 minutes without traffic, and the next visit wakes it up in a few seconds; a Basic dyno never sleeps.
- **More dynos**: running several dynos of the same process requires Standard dynos or higher.
- **Stop**: in the same edit mode, switch the `web` dyno off, or delete the app (**Settings** → **Delete app**).

### Custom domain

In **Settings** → **Domains**, tap **Add domain** and enter, for example, `www.example.com`. Heroku gives you a **DNS target**: create a `CNAME` record pointing to it at your domain registrar. A root domain (`example.com`) needs a registrar that supports `ALIAS`/`ANAME` records or CNAME flattening.

HTTPS certificates for custom domains are handled by Heroku's Automated Certificate Management (ACM), which is not available on every dyno type: check https://devcenter.heroku.com/articles/automated-certificate-management. More details: https://devcenter.heroku.com/articles/custom-domains.

## 5. Troubleshooting

### The build fails

Open the build log: in GitHub (**Actions** tab, failed run) or on Heroku (**Activity** tab → build log). Look for the first line that starts with `error`:

| Message | Cause | Fix |
|---|---|---|
| `error[E…]` with a file and a line | Rust compilation error | Fix the code at that line and commit again. |
| `no example target named …` | `ARG EXAMPLE` doesn't match a file in `examples/` | Use the file name without `.rs`. |
| `no matching package named …` or `failed to select a version …` | A crate name or version in `Cargo.toml` is wrong | Check the exact name and an existing version on https://crates.io, then commit again. |

### The workflow fails before building

| Message | Fix |
|---|---|
| `Heroku deployment is not configured` | Add the `HEROKU_API_KEY` secret and the `HEROKU_APP_NAME` variable (step 2). |
| `Heroku rejected the API key` | The token is wrong, expired or revoked: create a new one and update the secret (**Settings** → **Secrets and variables** → **Actions** → `HEROKU_API_KEY`). |
| `This Heroku account cannot access the app` | The name is already taken by another Heroku account: choose another name in `HEROKU_APP_NAME`. |

If creating the app fails, check that your Heroku account has a payment method, or create the app with the Deploy button (or the Heroku dashboard) first and put its name in `HEROKU_APP_NAME`.

### "Application error" in the browser

The build succeeded but the app doesn't answer. Open the logs and look for an error code:

| Code | Meaning | What to do |
|---|---|---|
| `R10` Boot timeout | The app did not listen on `$PORT` within 60 seconds after starting. | Listen on `$PORT` (see above), never on a fixed port or on `127.0.0.1`. |
| `H10` App crashed | The process stopped. | Read the lines just above in the logs: a panic in `main`, an invalid or duplicate route (Vitesse panics at startup and names the faulty route), a fixed port… |
| `H14` No web dynos running | No dyno is running for the `web` process. | In **Resources**, check that the `web` dyno is switched on. |
| `H12` Request timeout | A request took more than 30 seconds. | Heroku cuts requests after 30 seconds: make the handler faster, or use `middleware::timeout(...)` to answer earlier with a clean `503`. |

A panic inside a handler does not crash the app: Vitesse turns it into a `500` response. All Heroku error codes are listed at https://devcenter.heroku.com/articles/error-codes.

### Other surprises

- **The first request is slow**: an Eco dyno was asleep; it wakes up on the first visit.
- **My data disappeared**: in-memory data is lost on every restart. Use a database.
- **`PORT=8080` in the `Dockerfile`**: that is only the default for other platforms. On Heroku, the `PORT` variable set at startup takes precedence.

## Learn more

- [Docker](docker.md): build and run the same image on your computer or on other platforms.
- [Going to production](production.md): limits, logs, graceful shutdown and security.
- [Server configuration](server.md): `app.run`, listening addresses, workers.
