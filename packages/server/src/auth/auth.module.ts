import { Module } from '@nestjs/common'
import { AuthController } from './auth.controller'
import { AuthService } from './auth.service'
import { JwtModule } from '@nestjs/jwt'
import { UserModule } from 'src/user/user.module'
import { SharedModule } from '@server-octopus/shared'

@Module({
    imports: [UserModule, JwtModule.register({}), SharedModule],
    controllers: [AuthController],
    providers: [AuthService],
    exports: [AuthService]
})
export class AuthModule {}
