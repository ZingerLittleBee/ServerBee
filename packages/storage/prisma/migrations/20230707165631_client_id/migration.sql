-- DropForeignKey
ALTER TABLE "Disk" DROP CONSTRAINT "Disk_client_id_fkey";

-- DropForeignKey
ALTER TABLE "Network" DROP CONSTRAINT "Network_client_id_fkey";

-- AlterTable
ALTER TABLE "Disk" ALTER COLUMN "client_id" DROP NOT NULL;

-- AlterTable
ALTER TABLE "Network" ALTER COLUMN "client_id" DROP NOT NULL;

-- AddForeignKey
ALTER TABLE "Disk" ADD CONSTRAINT "Disk_client_id_fkey" FOREIGN KEY ("client_id") REFERENCES "Client"("client_id") ON DELETE SET NULL ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "Network" ADD CONSTRAINT "Network_client_id_fkey" FOREIGN KEY ("client_id") REFERENCES "Client"("client_id") ON DELETE SET NULL ON UPDATE CASCADE;
