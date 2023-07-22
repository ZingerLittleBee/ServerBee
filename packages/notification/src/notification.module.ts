import { Module } from '@nestjs/common'
import { NotificationController } from './notification.controller'
import { NotificationService } from './notification.service'
import { MailerModule } from '@nestjs-modules/mailer'
import { ReactAdapter } from '@webtre/nestjs-mailer-react-adapter'
import { ConfigModule, ConfigService } from '@nestjs/config'
import { kSmtpHost, kSmtpPassword, kSmtpPort, kSmtpUsername } from './const'

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
                    secure: false,
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
        })
    ],
    controllers: [NotificationController],
    providers: [NotificationService]
})
export class NotificationModule {}
