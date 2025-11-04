# UDP Hole Punching Client — кратко

**Что это**

Короткий Rust-клиент для демонстрации UDP hole punching: регистрируется на рандеву‑сервере, получает публичный адрес пира, делает "punch" и обменивается текстовыми сообщениями.

**Как скомпилировать**

```bash
cargo build --release
# бинарник: target/release/udp-hole
```

**Как запустить**

Два инстанса (рекомендуется в разных сетях):

Терминал A:

```bash
./target/release/udp-hole --name a --peer b --rendezvous 45.151.30.139
```

Терминал B:

```bash
./target/release/udp-hole --name b --peer a --rendezvous 45.151.30.139
```

Или во время разработки:

```bash
cargo run -- --name a --peer b --rendezvous 45.151.30.139
```

**Запуск с логами**

По умолчанию лог `info`. Для подробного вывода:

```bash
RUST_LOG=info ./target/release/udp-hole --name a --peer b
RUST_LOG=debug ./target/release/udp-hole --name a --peer b
```

**Короткие примечания**

* Рандеву по умолчанию: `45.151.30.139` (поменяй флагом `--rendezvous`).
* Если не работает — включи `RUST_LOG=debug` и проверь логи клиента/сервера.

---
