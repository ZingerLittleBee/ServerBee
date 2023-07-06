import { Module } from '@nestjs/common'
import { UtilModule } from '@/utils/util.module'
import { DbModule } from '@/db/db.module'

@Module({
    imports: [DbModule, UtilModule],
    exports: [DbModule, UtilModule]
})
export class SharedModule {}
