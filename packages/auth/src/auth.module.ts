import { Module } from '@nestjs/common'
import { ConfigModule } from '@nestjs/config'
import { TokenModule } from '@/token/token.module'

@Module({
    imports: [
        ConfigModule.forRoot({
            isGlobal: true
        }),
        TokenModule
    ]
})
export class AuthModule {}
