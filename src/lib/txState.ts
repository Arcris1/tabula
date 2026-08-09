import { stripComments } from "./railChecks";

export type TxDialect = "mysql" | "other";

/** MySQL DDL that implicitly commits any open transaction. */
const MYSQL_IMPLICIT_COMMIT = new Set(["create", "alter", "drop", "truncate", "rename"]);

/** How a successfully executed statement changes the session's transaction state. */
export function txEffect(sql: string, dialect: TxDialect = "other"): "begin" | "end" | null {
  const words = stripComments(sql, { hashComments: dialect === "mysql" })
    .trim().toLowerCase().replace(/;+\s*$/, "").split(/\s+/);
  const first = words[0] ?? "";
  if (first === "begin") {
    // Only a real transaction opener counts: bare BEGIN / BEGIN WORK /
    // BEGIN TRAN[SACTION] / BEGIN ISOLATION…. A T-SQL BEGIN…END *block*
    // (BEGIN PRINT…, BEGIN TRY…, BEGIN DECLARE…) must not light the TX badge.
    const next = words[1] ?? "";
    const opener = ["", "work", "transaction", "tran", "isolation", "read", "not", "deferrable"];
    return opener.includes(next) ? "begin" : null;
  }
  if (first === "start" && words[1] === "transaction") return "begin";
  if (first === "commit" || first === "end") {
    // COMMIT AND CHAIN commits but immediately opens a new transaction
    return words[1] === "and" && words[2] === "chain" ? null : "end";
  }
  if (first === "abort") return "end";
  if (first === "rollback") {
    // ROLLBACK TO [SAVEPOINT] x keeps the transaction open
    if (words[1] === "to") return null;
    return words[1] === "and" && words[2] === "chain" ? null : "end";
  }
  if (dialect === "mysql" && MYSQL_IMPLICIT_COMMIT.has(first)) return "end";
  return null;
}
