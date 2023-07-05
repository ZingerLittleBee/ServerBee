import { Module } from '@nestjs/common'
import { ErrorService } from '@app/shared/utils/error.util'
import { JwtUtilService } from '@app/shared/utils/jwt.util.service'
import { ConfigModule } from '@nestjs/config'

@Module({
    imports: [ConfigModule],
    providers: [JwtUtilService, ErrorService],
    exports: [JwtUtilService, ErrorService]
})
export class UtilModule {}
