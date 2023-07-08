import { Module } from '@nestjs/common'
import { ConfigModule } from '@nestjs/config'
import { JwtUtilService } from '@/utils/jwt.util.service'
import { ErrorService } from '@/utils/error.util'

@Module({
    imports: [ConfigModule],
    providers: [JwtUtilService, ErrorService],
    exports: [JwtUtilService, ErrorService]
})
export class UtilModule {}
