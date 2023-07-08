import { Module } from '@nestjs/common'
import { UtilModule } from '@/utils/util.module'

@Module({
    imports: [UtilModule],
    exports: [UtilModule]
})
export class SharedModule {}
