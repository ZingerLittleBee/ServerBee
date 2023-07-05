import { Module } from '@nestjs/common'
import { DbModule } from '@app/shared/db/db.module'
import { UtilModule } from '@app/shared/utils/util.module'

@Module({
    imports: [DbModule, UtilModule],
    exports: [DbModule, UtilModule]
})
export class SharedModule {}
