import { NestFactory } from '@nestjs/core'
import { AuthModule } from './auth.module'
import { MicroserviceOptions, Transport } from '@nestjs/microservices'
import { ConfigService } from '@nestjs/config'
import { defaultNatsServerUrl, kNatsServerUrl } from '@server-octopus/shared'

async function bootstrap() {
    const app = await NestFactory.create(AuthModule)
    const configService = app.get(ConfigService)
    const url =
        configService.get<string>(kNatsServerUrl) ?? defaultNatsServerUrl

    const microservice = app.connectMicroservice<MicroserviceOptions>({
        transport: Transport.NATS,
        options: {
            url
        }
    })
    await microservice.listen()
}
bootstrap()
