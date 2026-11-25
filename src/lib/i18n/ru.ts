import type { TranslationKey } from "./en";

// Russian catalogue. Typed as a full Record so a missing key is a compile error.
// Same blocks, same order as en.ts.
export const ru: Record<TranslationKey, string> = {
  // --- common.* (W0) ---
  "common.ok": "ОК",
  "common.cancel": "Отмена",
  "common.delete": "Удалить",
  "common.undo": "Отменить",
  "common.close": "Закрыть",
  "common.save": "Сохранить",
  "common.retry": "Повторить",
  "common.clear": "Очистить",
  "common.dismiss": "Скрыть",
  "common.open": "Открыть",
  "common.copy": "Копировать",
  "common.paste": "Вставить",
  "common.add": "Добавить",
  "common.remove": "Убрать",
  "common.rename": "Переименовать",
  "common.back": "Назад",
  "common.next": "Далее",
  "common.skip": "Пропустить",
  "common.done": "Готово",
  "common.loading": "Загрузка…",
  "common.on": "Вкл.",
  "common.off": "Выкл.",

  "common.error.not_found": "Этой записи больше нет.",
  "common.error.busy": "Буфер обмена или база заняты. Повторите через мгновение.",
  "common.error.invalid": "Недопустимое значение.",
  "common.error.failed": "Что-то пошло не так. Повторите.",
  "common.error.unimplemented": "Пока недоступно.",
  "common.error.unknown": "Что-то пошло не так.",

  "common.hotkey.taken": "Это сочетание занято другим приложением.",
  "common.hotkey.invalid": "Это сочетание нельзя использовать.",

  "common.kind.text": "Текст",
  "common.kind.link": "Ссылка",
  "common.kind.code": "Код",
  "common.kind.color": "Цвет",
  "common.kind.contact": "Контакт",
  "common.kind.image": "Картинка",
  "common.kind.file": "Файл",

  "common.size.bytes": "{n} Б",
  "common.size.kb": "{n} КБ",
  "common.size.mb": "{n} МБ",
  "common.size.gb": "{n} ГБ",

  // --- picker.* (W1-E) ---
  "picker.search": "Поиск",
  "picker.empty.title": "Скопируйте что-нибудь — оно появится здесь",
  "picker.empty.desc": "Alt + V открывает Liqlip из любого окна.",
  "picker.error.title": "Не удалось загрузить историю",

  // --- settings.* (W1-F) ---
  "settings.nav.label": "Разделы настроек",
  "settings.section.general": "Общие",
  "settings.section.picker": "Шелф",
  "settings.section.hotkeys": "Горячие клавиши",
  "settings.section.storage": "Хранение",
  "settings.section.privacy": "Приватность",
  "settings.section.snippets": "Сниппеты",
  "settings.section.ocr": "Распознавание текста",
  "settings.section.import": "Импорт и экспорт",
  "settings.section.about": "О Liqlip",

  // --- onboarding.* (W1-F) ---

  // --- hud.* (W1-D) ---
  "hud.copied": "Скопировано — нажмите {keys}",
  "hud.elevated": "Окно администратора — нажмите {keys}",
  "hud.stackEmpty": "Очередь пуста",
  "hud.paused": "Запись на паузе",
  "hud.resumed": "Запись возобновлена",
  "hud.skipNext": "Следующее копирование не сохранится",
  "hud.pasteKeys": "Ctrl+V",
};
