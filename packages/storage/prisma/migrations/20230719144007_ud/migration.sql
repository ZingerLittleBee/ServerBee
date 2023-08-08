/*
  Warnings:

  - You are about to drop the column `device_id` on the `Ud` table. All the data in the column will be lost.
  - A unique constraint covering the columns `[ud_id]` on the table `Ud` will be added. If there are existing duplicate values, this will fail.
  - The required column `ud_id` was added to the `Ud` table with a prisma-level default value. This is not possible if the table is not empty. Please add this column as optional, then populate it before making it required.

*/
-- DropIndex
DROP INDEX "Ud_device_id_key";

-- AlterTable
ALTER TABLE "Ud" DROP COLUMN "device_id",
ADD COLUMN     "ud_id" TEXT NOT NULL;

-- CreateIndex
CREATE UNIQUE INDEX "Ud_ud_id_key" ON "Ud"("ud_id");
