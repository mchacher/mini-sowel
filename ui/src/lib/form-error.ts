import type { TFunction } from "i18next";
import { ApiError } from "../api/client";
import { apiErrorText } from "./api-error";

/**
 * The text a form shows under its fields. A 404 carries the server's own words
 * (`Zone not found: <id>`, `Equipment not found`) which say which thing is missing; the generic
 * "does not exist" text stays where the server wrote nothing.
 */
export function formErrorText(err: unknown, t: TFunction): string {
  if (err instanceof ApiError && err.status === 404 && err.fromServer && err.message) {
    return err.message;
  }
  return apiErrorText(err, t);
}
