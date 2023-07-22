import { Module } from '@nestjs/common'
import { MailerModule } from '@nestjs-modules/mailer'
import { ReactAdapter } from '@webtre/nestjs-mailer-react-adapter'
import { ConfigModule, ConfigService } from '@nestjs/config'
import { kSmtpHost, kSmtpPassword, kSmtpPort, kSmtpUsername } from './const'
import { EmailModule } from './email/email.module'

@Module({
    imports: [
        ConfigModule.forRoot({ isGlobal: true }),
        MailerModule.forRootAsync({
            imports: [ConfigModule],
            inject: [ConfigService],
            useFactory: (configService: ConfigService) => ({
                transport: {
                    host: configService.getOrThrow(kSmtpHost),
                    port: configService.getOrThrow(kSmtpPort),
                    secure: true,
                    auth: {
                        user: configService.getOrThrow(kSmtpUsername),
                        pass: configService.getOrThrow(kSmtpPassword)
                    }
                },
                defaults: {
                    from: '"ServerBee" <support@serverbee.app>'
                },
                template: {
                    dir: __dirname + '/templates',
                    adapter: new ReactAdapter()
                }
            })
        }),
        EmailModule
    ]
})
export class NotificationModule {}
