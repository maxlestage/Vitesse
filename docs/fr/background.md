# Tâches de fond et planifiées

Certains travaux n'ont pas leur place dans la requête : envoyer un e-mail, redimensionner une image, appeler une API lente, purger les vieilles sessions chaque nuit. Vitesse a une file de tâches, `vitesse::queue` (les queues de Laravel, BullMQ, Sidekiq), et un planificateur, `vitesse::schedule` (le scheduler de Laravel, cron), tous deux en mémoire et tous deux à la demande.

## La file de tâches

Une `Queue` exécute des tâches en arrière-plan, avec un plafond de tâches simultanées, une limite de débit facultative et des nouvelles tentatives :

```rust
use std::time::Duration;
use vitesse::queue::Queue;

let queue = Queue::builder()
    .workers(4) // 4 tâches à la fois au plus (par défaut)
    .rate_limit(10, Duration::from_secs(1)) // 10 démarrages par seconde au plus
    .retries(3) // une tâche en échec est relancée jusqu'à 3 fois…
    .backoff(Duration::from_secs(1)) // …après 1 s, puis 2 s, puis 4 s
    .start();

let mut app = App::new();
app.state(queue);
```

`Queue::new()` démarre une file avec les réglages par défaut : 4 workers, sans nouvelle tentative ni limite de débit.

## Envoyer des tâches

Une tâche est une fonction qui renvoie un future, rappelée à chaque nouvelle tentative. Elle ne renvoie rien, ou un `Result` dont l'erreur est journalisée et retentée :

```rust
async fn send_welcome_email(to: String) -> Result<(), std::io::Error> {
    // … parler au serveur de messagerie
    Ok(())
}

app.post("/signup", |req: Request| async move {
    let email = req.text().await?;
    req.state::<Queue>().dispatch(move || send_welcome_email(email.clone()));
    Ok::<_, Error>((202, "Bienvenue ! Regardez votre boîte de réception."))
});
```

- `dispatch_after(délai, tâche)` démarre la tâche après un délai au plus tôt.
- Une tâche qui panique compte comme un échec : elle est retentée comme une erreur.
- `queue.pending()` compte les tâches en attente ou en cours, et `queue.idle().await` attend qu'il n'y en ait plus, pratique dans les tests avant de vérifier ce qu'elles ont fait.

## Les tâches planifiées

Un `Scheduler` exécute des fonctions à intervalles réguliers, en arrière-plan du serveur :

```rust
use std::time::Duration;
use vitesse::schedule::Scheduler;

fn main() -> std::io::Result<()> {
    let scheduler = Scheduler::new()
        .every(Duration::from_secs(30), "heartbeat", || async { println!("en vie") })
        .every_minute("refresh rates", refresh_exchange_rates)
        .hourly_at(5, "purge sessions", purge_sessions) // à hh:05 chaque heure
        .daily_at(3, 30, "nightly report", send_report) // à 03:30 UTC chaque jour
        .start();

    let mut app = App::new();
    app.get("/", |_| async { "Bonjour !" });
    app.run(3000)?;
    scheduler.stop();
    Ok(())
}
```

- Comme les tâches de la file, une tâche planifiée ne renvoie rien ou un `Result` ; les erreurs et les paniques sont journalisées, et la tâche repasse la fois suivante.
- Une tâche encore en cours quand vient son exécution suivante n'est pas lancée deux fois (le `withoutOverlapping` de Laravel).
- Les heures sont en UTC. `every(intervalle, …)` s'exécute une première fois un intervalle après le démarrage.
- `scheduler.stop()` arrête les tâches ; abandonner la poignée les laisse tourner.

## Où elles s'exécutent

La file et le planificateur tournent sur le runtime tokio courant quand il y en a un (`#[tokio::main]`, `app.listen`, les tests), et sinon sur un thread à eux avec son propre runtime : créez-les avant `app.run(…)` et ils tournent à côté du serveur.

## Limites

Les tâches et les planifications vivent dans la mémoire du processus : les tâches encore en attente sont perdues s'il s'arrête, et chaque instance de l'application a son propre planificateur. Pour des tâches qui doivent survivre à un redémarrage, utilisez une file adossée à une base de données ou à un courtier ; avec plusieurs instances, lancez le planificateur dans une seule.
