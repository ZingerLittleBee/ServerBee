/*
  Warnings:

  - You are about to drop the column `localtion` on the `Ud` table. All the data in the column will be lost.

*/
-- AlterTable
ALTER TABLE "Ud" DROP COLUMN "localtion",
ADD COLUMN     "location" TEXT;
