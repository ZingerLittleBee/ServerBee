import { Module } from '@nestjs/common'
import { AuthController } from './auth.controller'
import { AuthService } from './auth.service'
import { UserModule } from '@/user/user.module'
import { JwtModule } from '@nestjs/jwt'
import { UtilModule } from '@/utils/util.module'

@Module({
    imports: [UserModule, JwtModule.register({}), UtilModule],
    controllers: [AuthController],
    providers: [AuthService],
    exports: [AuthService]
})
export class AuthModule {}
