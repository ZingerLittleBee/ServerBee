import { Module } from '@nestjs/common'
import { TokenController } from '@/token/token.controller'
import { TokenService } from '@/token/token.service'
import { JwtModule } from '@nestjs/jwt'
import { TokenUtilService } from '@/token/token.util.service'

@Module({
    imports: [JwtModule.register({})],
    controllers: [TokenController],
    providers: [TokenService, TokenUtilService]
})
export class TokenModule {}
