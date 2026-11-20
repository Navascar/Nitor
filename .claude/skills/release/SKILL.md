---
name: release
description: Випустити нову версію Nitor — версія, CHANGELOG, тести, перевірка на Windows, злиття в main і перевірка релізу на GitHub.
disable-model-invocation: true
---

# Випуск версії Nitor

Реліз іде всім користувачам (програма сама пропонує оновлення), тож дія незворотна.
Аргумент (необов'язковий): номер версії, напр. `/release 2.1.0`. Без нього — наступна patch-версія.

## 1. Підготовка у гілці

1. Переконайтеся, що зміни закомічені в гілці (не `main`): `git status -sb`.
2. Номер версії має бути **вищим за будь-який колишній реліз**:
   ```bash
   gh api "repos/Navascar/Nitor/releases?per_page=5" -q '.[].tag_name'
   grep -A3 '^\[workspace.package\]' Cargo.toml
   ```
3. Оновіть `version` у `[workspace.package]` кореневого `Cargo.toml` (і `Cargo.lock` — `cargo check`).
4. Додайте на початок `CHANGELOG.md` розділ `## X.Y.Z`: короткі пункти українською для користувача — що змінилося
   й що це дає, без технічних подробиць. Цей текст стане описом релізу й вікном «Що нового».
5. Перевірки:
   ```bash
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

## 2. Перевірка на Windows

1. Закомітьте й запуште гілку: `git push -u origin <гілка>`.
2. Дочекайтеся `Windows check` для цього коміту:
   ```bash
   gh run list -R Navascar/Nitor -b <гілка> -L 1 --json databaseId,status,conclusion,headSha
   ```
   Якщо щось упало — `gh run view <id> --log-failed`. Знімки вікон — рядки `SHOT|назва|base64` у журналі.

## 3. Підтвердження

Покажіть користувачу підсумок (версія, пункти CHANGELOG, результати тестів і Windows check) і **спитайте, чи
випускати**. Без явного «так» у `main` не пушити.

## 4. Випуск

```bash
git fetch --depth=50 origin main
git checkout main && git merge --ff-only <гілка>
git push origin main
```
Дочекайтеся `Tests & Release` для цього коміту й перевірте реліз:
```bash
gh api repos/Navascar/Nitor/releases/latest -q '"\(.tag_name) " + ([.assets[].name] | join(", "))'
```
У релізі має бути `Nitor-Setup.exe`. Повідомте користувачу посилання на реліз.
