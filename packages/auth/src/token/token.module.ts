import { Module } from '@nestjs/common'
import { TokenController } from '@/token/token.controller'
import { TokenService } from '@/token/token.service'
import { TokenUtilService } from '@/token/token.util.service'

@Module({
    controllers: [TokenController],
    providers: [TokenService, TokenUtilService],
    exports: [TokenService, TokenUtilService]
})
export class TokenModule {}
