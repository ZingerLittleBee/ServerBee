/*
  Warnings:

  - The values [WEB] on the enum `Type` will be removed. If these variants are still used in the database, this will fail.

*/
-- AlterEnum
BEGIN;
CREATE TYPE "Type_new" AS ENUM ('UNKNOWN', 'DESKTOP', 'TABLET', 'MOBILE', 'CLIENT');
ALTER TABLE "Ud" ALTER COLUMN "type" DROP DEFAULT;
ALTER TABLE "Ud" ALTER COLUMN "type" TYPE "Type_new" USING ("type"::text::"Type_new");
ALTER TYPE "Type" RENAME TO "Type_old";
ALTER TYPE "Type_new" RENAME TO "Type";
DROP TYPE "Type_old";
ALTER TABLE "Ud" ALTER COLUMN "type" SET DEFAULT 'UNKNOWN';
COMMIT;
