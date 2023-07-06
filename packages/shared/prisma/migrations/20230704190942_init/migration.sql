-- AlterTable
ALTER TABLE "Device" ADD COLUMN     "version" TEXT;

-- CreateTable
CREATE TABLE "Disk" (
    "id" SERIAL NOT NULL,
    "device_id" INTEGER NOT NULL,
    "name" TEXT,
    "type" TEXT,
    "size" TEXT,
    "used" TEXT,
    "created_at" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMP(3) NOT NULL,

    CONSTRAINT "Disk_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "Network" (
    "id" SERIAL NOT NULL,
    "device_id" INTEGER NOT NULL,
    "name" TEXT,
    "mac" TEXT,
    "rx" TEXT,
    "tx" TEXT,
    "created_at" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMP(3) NOT NULL,

    CONSTRAINT "Network_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE UNIQUE INDEX "Disk_device_id_key" ON "Disk"("device_id");

-- CreateIndex
CREATE UNIQUE INDEX "Network_device_id_key" ON "Network"("device_id");

-- AddForeignKey
ALTER TABLE "Disk" ADD CONSTRAINT "Disk_device_id_fkey" FOREIGN KEY ("device_id") REFERENCES "Device"("id") ON DELETE RESTRICT ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "Network" ADD CONSTRAINT "Network_device_id_fkey" FOREIGN KEY ("device_id") REFERENCES "Device"("id") ON DELETE RESTRICT ON UPDATE CASCADE;
