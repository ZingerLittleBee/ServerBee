import { Module } from '@nestjs/common'
import { UtilModule } from '@/utils/util.module'
import { DbModule } from '@/db/db.module'
import { ConfigModule } from '@nestjs/config'
import { StorageController } from '@/storage.controller'
import { ProfileModule } from '@/profile/profile.module'
import { UserModule } from './user/user.module'
import { ClientModule } from './client/client.module';

@Module({
    imports: [
        ConfigModule.forRoot(),
        DbModule,
        UtilModule,
        ProfileModule,
        UserModule,
        ClientModule
    ],
    controllers: [StorageController]
})
export class StorageModule {}
