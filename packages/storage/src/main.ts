import { NestFactory } from '@nestjs/core'
import { StorageModule } from './storage.module'
import { MicroserviceOptions, Transport } from '@nestjs/microservices'
import { ConfigService } from '@nestjs/config'
import { defaultNatsServerUrl, kNatsServerUrl } from '@server-octopus/shared'

async function bootstrap() {
    const app = await NestFactory.create(StorageModule)
    const configService = app.get(ConfigService)
    const url =
        configService.get<string>(kNatsServerUrl) ?? defaultNatsServerUrl

    const microservice = app.connectMicroservice<MicroserviceOptions>({
        transport: Transport.NATS,
        options: {
            servers: [url]
        }
    })
    await microservice.listen()
}
bootstrap()
