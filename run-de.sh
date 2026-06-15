#!/bin/bash

# Сборка всех компонентов в режиме дебага
echo "[System] Building workspace..."
cargo build

# Очистка старых логов и сокетов
rm -f /tmp/my-de-ipc.sock

# 1. Запуск композитора (в фоновом режиме, сохраняя вывод в логи)
echo "[System] Launching de-compositor..."
./target/debug/de-compositor > compositor.log 2>&1 &
COMPOSITOR_PID=$!

# Даем сокету 1 секунду на создание файла в /tmp/
sleep 1

# 2. Запуск демона-менеджера в фоновом режиме
echo "[System] Launching de-manager..."
./target/debug/de-manager &
MANAGER_PID=$!

sleep 0.5

# 3. Запуск GUI панели в основном потоке (блокирующий вызов)
echo "[System] Launching de-panel..."
./target/debug/de-panel

# При закрытии окна панели — аккуратно гасим все фоновые сервисы
echo "[System] Panel closed. Shutting down DE components..."
kill $COMPOSITOR_PID
kill $MANAGER_PID

echo "[System] DE successfully stopped."