import en from "./en.json";

export type StringKeys = keyof typeof en;

const strings: Record<string, string> = en;

export function t(
  key: StringKeys,
  params?: Record<string, string | number>,
): string {
  const value = strings[key];
  if (value === undefined) {
    console.warn(`Missing i18n key: ${key}`);
    return key;
  }
  if (!params) return value;
  return value.replace(/\{(\w+)\}/g, (_, name: string) =>
    name in params ? String(params[name]) : `{${name}}`,
  );
}
