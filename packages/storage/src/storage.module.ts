import { Module } from '@nestjs/common'
import { UtilModule } from '@/utils/util.module'
import { DbModule } from '@/db/db.module'
import { ConfigModule } from '@nestjs/config'

@Module({
    imports: [ConfigModule.forRoot(), DbModule, UtilModule]
})
export class StorageModule {}
