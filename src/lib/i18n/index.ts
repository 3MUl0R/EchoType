import en from "./en.json";

type StringKeys = keyof typeof en;

const strings: Record<string, string> = en;

export function t(key: StringKeys): string {
  const value = strings[key];
  if (value === undefined) {
    console.warn(`Missing i18n key: ${key}`);
    return key;
  }
  return value;
}
