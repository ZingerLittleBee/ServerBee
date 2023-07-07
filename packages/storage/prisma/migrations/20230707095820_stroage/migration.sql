/*
  Warnings:

  - You are about to drop the column `size` on the `Disk` table. All the data in the column will be lost.
  - You are about to drop the column `used` on the `Disk` table. All the data in the column will be lost.

*/
-- AlterTable
ALTER TABLE "Disk" DROP COLUMN "size",
DROP COLUMN "used",
ADD COLUMN     "available" TEXT,
ADD COLUMN     "file_system" TEXT,
ADD COLUMN     "removeable" BOOLEAN,
ADD COLUMN     "total" TEXT;
