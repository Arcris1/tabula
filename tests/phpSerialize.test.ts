import { describe, it, expect } from "vitest";
import { phpUnserialize, tryPhpFormat } from "../src/lib/phpSerialize";

describe("phpUnserialize", () => {
  it("parses scalars", () => {
    expect(phpUnserialize("i:42;").value).toBe(42);
    expect(phpUnserialize("b:1;").value).toBe(true);
    expect(phpUnserialize("b:0;").value).toBe(false);
    expect(phpUnserialize("N;").value).toBe(null);
    expect(phpUnserialize("d:1.5;").value).toBe(1.5);
    expect(phpUnserialize('s:5:"hello";').value).toBe("hello");
  });

  it("string lengths are BYTES (multibyte-safe)", () => {
    // "héllo" is 6 bytes in UTF-8
    expect(phpUnserialize('s:6:"héllo";').value).toBe("héllo");
  });

  it("sequential arrays become JS arrays, keyed ones objects", () => {
    expect(phpUnserialize('a:2:{i:0;s:1:"a";i:1;s:1:"b";}').value).toEqual(["a", "b"]);
    expect(phpUnserialize('a:1:{s:3:"foo";i:7;}').value).toEqual({ foo: 7 });
  });

  it("parses nested structures like a Laravel session", () => {
    const v = phpUnserialize(
      'a:2:{s:6:"_token";s:4:"tok1";s:6:"_flash";a:2:{s:3:"old";a:0:{}s:3:"new";a:0:{}}}',
    ).value as any;
    expect(v._token).toBe("tok1");
    expect(v._flash).toEqual({ old: [], new: [] });
  });

  it("rejects non-serialized text and trailing garbage", () => {
    expect(phpUnserialize("hello world").ok).toBe(false);
    expect(phpUnserialize('{"json": true}').ok).toBe(false);
    expect(phpUnserialize("i:42;EXTRA").ok).toBe(false);
    expect(phpUnserialize('s:99:"short";').ok).toBe(false);
  });

  it("objects keep class name, visibility markers stripped", () => {
    const v = phpUnserialize('O:3:"Foo":1:{s:3:"bar";i:1;}').value as any;
    expect(v.__class).toBe("Foo");
    expect(v.bar).toBe(1);
  });
});

describe("tryPhpFormat (nested resolution)", () => {
  it("unwraps Laravel's double-serialized session payload", () => {
    const inner = 'a:2:{s:6:"_token";s:4:"tok1";s:16:"selected_account";i:-690;}';
    const bytes = new TextEncoder().encode(inner).length;
    const outer = `s:${bytes}:"${inner}";`;
    const v = tryPhpFormat(outer) as any;
    expect(v._token).toBe("tok1");
    expect(v.selected_account).toBe(-690);
  });

  it("returns null for plain text", () => {
    expect(tryPhpFormat("just a value")).toBeNull();
  });
});
