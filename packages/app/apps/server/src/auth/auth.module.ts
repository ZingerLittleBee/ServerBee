import { Module } from '@nestjs/common'
import { AuthController } from './auth.controller'
import { AuthService } from './auth.service'
import { JwtModule } from '@nestjs/jwt'
import { SharedModule } from '@app/shared'
import { UserModule } from '@app/server/user/user.module'

@Module({
    imports: [UserModule, JwtModule.register({}), SharedModule],
    controllers: [AuthController],
    providers: [AuthService],
    exports: [AuthService]
})
export class AuthModule {}
