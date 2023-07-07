/*
  Warnings:

  - Added the required column `client_id` to the `Disk` table without a default value. This is not possible if the table is not empty.
  - Added the required column `client_id` to the `Network` table without a default value. This is not possible if the table is not empty.

*/
-- DropIndex
DROP INDEX "Client_user_id_key";

-- DropIndex
DROP INDEX "Disk_device_id_key";

-- DropIndex
DROP INDEX "Network_device_id_key";

-- AlterTable
ALTER TABLE "Disk" ADD COLUMN     "client_id" TEXT NOT NULL;

-- AlterTable
ALTER TABLE "Network" ADD COLUMN     "client_id" TEXT NOT NULL;

-- AddForeignKey
ALTER TABLE "Disk" ADD CONSTRAINT "Disk_client_id_fkey" FOREIGN KEY ("client_id") REFERENCES "Client"("client_id") ON DELETE RESTRICT ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "Network" ADD CONSTRAINT "Network_client_id_fkey" FOREIGN KEY ("client_id") REFERENCES "Client"("client_id") ON DELETE RESTRICT ON UPDATE CASCADE;
