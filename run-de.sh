#!/bin/bash

# Сборка всех компонентов
echo "[System] Building workspace..."
cargo build

# Скопируем свежесобранный модуль часов в папку расширений (на случай, если вы меняли код)
echo "[System] Updating clock extension binary..."
mkdir -p ~/.local/share/my-de/extensions/clock@my-de.org
cp target/debug/mod-clock ~/.local/share/my-de/extensions/clock@my-de.org/
cat <<EOF > ~/.local/share/my-de/extensions/clock@my-de.org/metadata.json
{
  "uuid": "clock@my-de.org",
  "name": "Date & Time",
  "description": "Displays clock in the top panel",
  "exec": "./mod-clock",
  "version": "1.0.0"
}
EOF

# Очистка старых сокетов
rm -f /tmp/my-de-ipc.sock

# 1. Запуск композитора (в фоне)
echo "[System] Launching de-compositor..."
./target/debug/de-compositor > compositor.log 2>&1 &
COMPOSITOR_PID=$!

sleep 1

# 2. Запуск демона-менеджера (в фоне)
# Теперь он сам просканирует ~/.local/share/my-de/extensions/
echo "[System] Launching de-manager..."
./target/debug/de-manager &
MANAGER_PID=$!

sleep 0.5

# 3. Запуск GUI панели на GTK4 (в основном потоке)
echo "[System] Launching GTK4 de-panel..."
./target/debug/de-panel

# При закрытии окна панели аккуратно гасим фоновые сервисы
echo "[System] Panel closed. Shutting down DE components..."
kill $MANAGER_PID # Благодаря нашему Drop() он убьет и все дочерние расширения!
kill $COMPOSITOR_PID

echo "[System] DE successfully stopped."